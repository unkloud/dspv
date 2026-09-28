// SPDX-License-Identifier: MIT

use crate::config::Config;
use cosmic::cosmic_config::{self, CosmicConfigEntry};
use cosmic::iced::platform_specific::shell::wayland::commands::popup::{destroy_popup, get_popup};
use cosmic::iced::{window::Id, Limits, Subscription};
use cosmic::prelude::*;
use cosmic::widget;
use chrono::{DateTime, Datelike, NaiveDate, Utc, Timelike, Duration};
use std::sync::LazyLock;
use serde::{Deserialize, Serialize};

static AUTOSIZE_MAIN_ID: LazyLock<cosmic::widget::Id> = LazyLock::new(|| cosmic::widget::Id::new("cosmic-applet-autosize-main"));
const FALLBACK_RULES_JSON: &str = include_str!("../resources/pricing_rules.json");

#[derive(Debug, Serialize, Deserialize, Clone)]
struct Peak {
    start_hour: u32,
    end_hour: u32,
    rate: f32,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct Promotion {
    end_date: String,
    off_peak_rate: f32,
}

/// One all-day off-peak window in the vendor's local calendar. `start` is
/// inclusive and `end` is exclusive, so a single day is `end == start + 1 day`.
#[derive(Debug, Serialize, Deserialize, Clone)]
struct Exception {
    start: NaiveDate,
    end: NaiveDate,
}

impl Exception {
    fn contains(&self, date: NaiveDate) -> bool {
        date >= self.start && date < self.end
    }
}

/// Accepts either `"YYYY-MM-DD"` for a single day or `["START", "END"]` for a
/// start-inclusive/end-exclusive range. Written by hand because the two shapes
/// cannot be expressed by serde's derive.
fn deserialize_exceptions<'de, D>(deserializer: D) -> Result<Vec<Exception>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::Error as _;

    let raw = Vec::<serde_json::Value>::deserialize(deserializer)?;
    let mut out = Vec::with_capacity(raw.len());
    for value in raw {
        let (start, end) = match value {
            serde_json::Value::String(day) => {
                let start = NaiveDate::parse_from_str(&day, "%Y-%m-%d")
                    .map_err(|e| D::Error::custom(format!("exception {day:?}: {e}")))?;
                (start, start + Duration::days(1))
            }
            serde_json::Value::Array(days) => {
                if days.len() != 2 {
                    return Err(D::Error::custom(
                        "exception range must be [\"START\", \"END\"]",
                    ));
                }
                let parse = |v: &serde_json::Value| {
                    v.as_str()
                        .ok_or_else(|| D::Error::custom("exception range entries must be strings"))
                        .and_then(|s| {
                            NaiveDate::parse_from_str(s, "%Y-%m-%d")
                                .map_err(|e| D::Error::custom(format!("exception {s:?}: {e}")))
                        })
                };
                (parse(&days[0])?, parse(&days[1])?)
            }
            _ => {
                return Err(D::Error::custom(
                    "exception must be a \"YYYY-MM-DD\" string or [\"START\", \"END\"] range",
                ));
            }
        };
        if end <= start {
            return Err(D::Error::custom(format!(
                "exception range {}..{} must be non-empty and increasing",
                start, end
            )));
        }
        out.push(Exception { start, end });
    }
    Ok(out)
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct Vendor {
    name: String,
    icon: String,
    timezone_offset_hours: i32,
    activation_date: Option<String>,
    default_rate: f32,
    peaks: Vec<Peak>,
    /// ISO weekday numbers (1 = Monday .. 7 = Sunday) on which peak rates may
    /// apply. Missing/empty means every day, which is the historical behaviour.
    #[serde(default)]
    days_of_week: Option<Vec<u32>>,
    /// Local calendar windows billed as off-peak all day, e.g. public holidays.
    #[serde(default, deserialize_with = "deserialize_exceptions")]
    exceptions: Vec<Exception>,
    promotion: Option<Promotion>,
}

impl Vendor {
    /// Whether peak rates are allowed at all on this local date.
    fn peak_enabled_on(&self, date: NaiveDate) -> bool {
        self.days_of_week.as_ref().is_none_or(|days| {
            days.contains(&date.weekday().number_from_monday())
        })
    }

    /// Whether this local date falls in an announced all-day off-peak window.
    fn in_exception(&self, date: NaiveDate) -> bool {
        self.exceptions.iter().any(|e| e.contains(date))
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct RulesConfig {
    vendors: Vec<Vendor>,
}

#[derive(Debug, Clone)]
pub struct VendorState {
    name: String,
    icon: String,
    is_peak: bool,
    rate: f32,
    countdown_str: String,
    next_change_time: DateTime<Utc>,
    off_peak_reason: OffPeakReason,
}

fn load_rules() -> RulesConfig {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/home/ew".to_string());
    let cache_dir = std::path::PathBuf::from(home).join(".config").join("tkmon");
    let cache_file = cache_dir.join("pricing_rules.json");

    // Rules are fetched at runtime so schedule changes (holidays, promotions,
    // new vendors) do not require a rebuild. A failed or older fetch degrades to
    // the cache and then to the rules bundled at build time.
    let default_url = "https://raw.githubusercontent.com/unkloud/dspv/main/resources/pricing_rules.json";

    // Attempt download with 2-second timeout
    let downloaded_content = ureq::get(default_url)
        .timeout(std::time::Duration::from_secs(2))
        .call()
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))
        .and_then(|res| res.into_string());

    let json_str = match downloaded_content {
        Ok(content) => {
            let _ = std::fs::create_dir_all(&cache_dir);
            let _ = std::fs::write(&cache_file, &content);
            content
        }
        Err(_) => {
            std::fs::read_to_string(&cache_file).unwrap_or_else(|_| {
                FALLBACK_RULES_JSON.to_string()
            })
        }
    };

    serde_json::from_str(&json_str).unwrap_or_else(|_| {
        serde_json::from_str(FALLBACK_RULES_JSON).expect("Fallback rules JSON must be valid")
    })
}

/// Why a vendor is currently off-peak, so the popup can say "holiday" instead
/// of implying a normal valley hour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OffPeakReason {
    Peak,
    Valley,
    /// An announced all-day off-peak window, e.g. a public holiday.
    Exception,
    /// Before `activation_date`: the policy is not in force yet.
    NotYetActive,
}

/// Outcome of evaluating one vendor's schedule at an instant.
#[derive(Debug, Clone)]
struct VendorStatus {
    is_peak: bool,
    rate: f32,
    next_change_time: DateTime<Utc>,
    countdown_str: String,
    off_peak_reason: OffPeakReason,
}

/// Rate in force during an off-peak period: a live promotion wins, else the
/// vendor's standing off-peak rate.
fn off_peak_rate(vendor: &Vendor, now: DateTime<Utc>) -> f32 {
    if let Some(ref promo) = vendor.promotion {
        if let Ok(promo_end) = DateTime::parse_from_rfc3339(&promo.end_date) {
            if now < promo_end.with_timezone(&Utc) {
                return promo.off_peak_rate;
            }
        }
    }
    vendor.default_rate
}

/// Days scanned for an upcoming transition. Wide enough that a Friday-evening
/// peak still finds Monday's start, and that a long holiday window always has a
/// clear exit.
const TRANSITION_LOOKAHEAD_DAYS: i64 = 14;

fn check_vendor_pricing(vendor: &Vendor, now: DateTime<Utc>) -> VendorStatus {
    if let Some(ref act_str) = vendor.activation_date {
        if let Ok(act_date) = DateTime::parse_from_rfc3339(act_str) {
            let act_utc = act_date.with_timezone(&Utc);
            if now < act_utc {
                return VendorStatus {
                    is_peak: false,
                    rate: vendor.default_rate,
                    next_change_time: act_utc,
                    countdown_str: format_duration(act_utc - now),
                    off_peak_reason: OffPeakReason::NotYetActive,
                };
            }
        }
    }

    let offset = Duration::hours(vendor.timezone_offset_hours as i64);
    let now_local = now + offset;
    let today = now_local.date_naive();
    let hour = now_local.hour();

    let in_exception = vendor.in_exception(today);

    let current_peak: Option<&Peak> = if vendor.peak_enabled_on(today) && !in_exception {
        vendor
            .peaks
            .iter()
            .find(|peak| peak.start_hour <= hour && hour < peak.end_hour)
    } else {
        None
    };

    let is_peak = current_peak.is_some();
    let (rate, off_peak_reason) = match current_peak {
        Some(peak) => (peak.rate, OffPeakReason::Peak),
        None if in_exception => (off_peak_rate(vendor, now), OffPeakReason::Exception),
        None => (off_peak_rate(vendor, now), OffPeakReason::Valley),
    };

    // Next instant the rate or peak/off-peak state changes. Peaks are the only
    // intra-day transitions; day boundaries matter only because the weekday and
    // exception rules are evaluated per local date, so each candidate is
    // re-tested with the rules of its own day.
    let mut transition_hours = std::collections::BTreeSet::new();
    for peak in &vendor.peaks {
        transition_hours.insert(peak.start_hour);
        transition_hours.insert(peak.end_hour);
    }

    if transition_hours.is_empty() {
        let next_hour = now + Duration::hours(1);
        return VendorStatus {
            is_peak,
            rate,
            next_change_time: next_hour,
            countdown_str: format_duration(next_hour - now),
            off_peak_reason,
        };
    }

    let base_today_local = now_local
        .with_minute(0)
        .unwrap()
        .with_second(0)
        .unwrap()
        .with_nanosecond(0)
        .unwrap();

    let mut next_change_utc = None;
    for day_offset in 0..TRANSITION_LOOKAHEAD_DAYS {
        let base_day_local = base_today_local + Duration::days(day_offset);
        let day = base_day_local.date_naive();
        let peaks_allowed = vendor.peak_enabled_on(day) && !vendor.in_exception(day);
        if !peaks_allowed {
            continue;
        }
        for &h in &transition_hours {
            if let Some(trans_local) = base_day_local.with_hour(h) {
                let trans_utc = trans_local - offset;
                if trans_utc > now && next_change_utc.is_none_or(|best| trans_utc < best) {
                    next_change_utc = Some(trans_utc);
                }
            }
        }
    }

    let next_change = next_change_utc.unwrap_or_else(|| now + Duration::hours(1));
    VendorStatus {
        is_peak,
        rate,
        next_change_time: next_change,
        countdown_str: format_duration(next_change - now),
        off_peak_reason,
    }
}

fn format_duration(duration: Duration) -> String {
    let days = duration.num_days();
    let hours = duration.num_hours() % 24;
    let minutes = duration.num_minutes() % 60;

    let mut parts = Vec::new();
    if days > 0 {
        parts.push(format!("{}D", days));
    }
    if hours > 0 || days > 0 {
        parts.push(format!("{}H", hours));
    }
    parts.push(format!("{}Min", minutes));

    parts.join(" ")
}

pub struct AppModel {
    core: cosmic::Core,
    popup: Option<Id>,
    config: Config,
    rules_config: RulesConfig,
    vendor_states: Vec<VendorState>,
    current_tab_index: usize,
    last_update: DateTime<Utc>,
    panel_label: String,
}

#[derive(Debug, Clone)]
pub enum Message {
    TogglePopup,
    PopupClosed(Id),
    UpdateConfig(Config),
    Tick,
    SetTabIndex(usize),
}

impl cosmic::Application for AppModel {
    type Executor = cosmic::executor::Default;
    type Flags = ();
    type Message = Message;
    const APP_ID: &'static str = "com.system76.CosmicAppletTkmon";

    fn core(&self) -> &cosmic::Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut cosmic::Core {
        &mut self.core
    }

    fn init(
        core: cosmic::Core,
        _flags: Self::Flags,
    ) -> (Self, Task<cosmic::Action<Self::Message>>) {
        let config = cosmic_config::Config::new(Self::APP_ID, Config::VERSION)
            .map(|context| match Config::get_entry(&context) {
                Ok(config) => config,
                Err((_errors, config)) => config,
            })
            .unwrap_or_default();

        let rules_config = load_rules();
        let now = Utc::now();

        let mut vendor_states = Vec::new();
        for v in &rules_config.vendors {
            let status = check_vendor_pricing(v, now);
            vendor_states.push(VendorState {
                name: v.name.clone(),
                icon: v.icon.clone(),
                is_peak: status.is_peak,
                rate: status.rate,
                countdown_str: status.countdown_str,
                next_change_time: status.next_change_time,
                off_peak_reason: status.off_peak_reason,
            });
        }

        let label_parts: Vec<String> = vendor_states.iter().map(|state| {
            format!("{} {}x • {}", state.icon, state.rate, state.countdown_str)
        }).collect();
        let panel_label = label_parts.join(" | ");

        let app = AppModel {
            core,
            popup: None,
            config,
            rules_config,
            vendor_states,
            current_tab_index: 0,
            last_update: now,
            panel_label,
        };

        (app, Task::none())
    }

    fn on_close_requested(&self, id: Id) -> Option<Message> {
        Some(Message::PopupClosed(id))
    }

    fn view(&self) -> Element<'_, Self::Message> {
        let horizontal = matches!(
            self.core.applet.anchor,
            cosmic::applet::cosmic_panel_config::PanelAnchor::Top | cosmic::applet::cosmic_panel_config::PanelAnchor::Bottom
        );

        let layout = if horizontal {
            Element::from(
                widget::row!(
                    self.core.applet.text(self.panel_label.as_str()),
                    widget::container(widget::space::vertical().height(cosmic::iced::Length::Fixed(
                        (self.core.applet.suggested_size(true).1
                            + 2 * self.core.applet.suggested_padding(true).1)
                            as f32
                    )))
                )
                .align_y(cosmic::iced::Alignment::Center)
            )
        } else {
            Element::from(
                widget::column!(
                    self.core.applet.text(self.panel_label.as_str()),
                    widget::container(widget::space::horizontal().width(cosmic::iced::Length::Fixed(
                        (self.core.applet.suggested_size(true).0
                            + 2 * self.core.applet.suggested_padding(true).0)
                            as f32
                    )))
                )
                .align_x(cosmic::iced::Alignment::Center)
            )
        };

        let button = widget::button::custom(layout)
            .padding(if horizontal {
                [0, self.core.applet.suggested_padding(true).0]
            } else {
                [self.core.applet.suggested_padding(true).0, 0]
            })
            .on_press_down(Message::TogglePopup)
            .class(cosmic::theme::Button::AppletIcon);

        eprintln!("DEBUG TKMON VIEW: panel_label = {}, width = {:?}", self.panel_label, self.panel_label.len());

        cosmic::widget::autosize::autosize(button, AUTOSIZE_MAIN_ID.clone()).into()
    }

    fn view_window(&self, _id: Id) -> Element<'_, Self::Message> {
        let now_local = self.last_update.with_timezone(&chrono::Local);

        let mut tab_row = widget::row!().spacing(4);
        for (idx, state) in self.vendor_states.iter().enumerate() {
            let btn = widget::button::text(&state.name)
                .on_press(Message::SetTabIndex(idx))
                .width(cosmic::iced::Length::Fill)
                .class(if self.current_tab_index == idx {
                    cosmic::theme::Button::Suggested
                } else {
                    cosmic::theme::Button::Text
                });
            tab_row = tab_row.push(btn);
        }

        let mut tab_content = widget::list_column();

        if let Some(state) = self.vendor_states.get(self.current_tab_index) {
            let switch_local = state.next_change_time.with_timezone(&chrono::Local);
            let status_str = match state.off_peak_reason {
                OffPeakReason::Peak => format!("📈 PEAK HOUR ({:.1}x price)", state.rate),
                OffPeakReason::Exception => {
                    format!("📉 OFF-PEAK — HOLIDAY/PROMO ({:.1}x price)", state.rate)
                }
                OffPeakReason::NotYetActive => {
                    format!("⏳ NOT YET ACTIVE ({:.1}x price)", state.rate)
                }
                OffPeakReason::Valley => format!("📉 VALLEY HOUR ({:.1}x price)", state.rate),
            };
            let next_state_str = if state.is_peak { "VALLEY HOUR" } else { "PEAK HOUR" };

            tab_content = tab_content
                .add(widget::settings::item(
                    "Status".to_string(),
                    widget::text(status_str).size(14),
                ))
                .add(widget::settings::item(
                    "Next Switch to".to_string(),
                    widget::text(next_state_str).size(13),
                ))
                .add(widget::settings::item(
                    "Time Remaining".to_string(),
                    widget::text(&state.countdown_str).size(13),
                ))
                .add(widget::settings::item(
                    "Switch Time (Local)".to_string(),
                    widget::text(switch_local.format("%Y-%m-%d %H:%M:%S %Z").to_string()).size(12),
                ));
        }

        tab_content = tab_content.add(widget::settings::item(
            "Last Updated".to_string(),
            widget::text(now_local.format("%H:%M:%S %Z").to_string()).size(12),
        ));

        let main_column = widget::column!(
            tab_row,
            widget::space::vertical().height(8),
            tab_content
        )
        .padding(8);

        self.core.applet.popup_container(main_column).into()
    }

    fn subscription(&self) -> Subscription<Self::Message> {
        Subscription::batch(vec![
            cosmic::iced::time::every(std::time::Duration::from_secs(10))
                .map(|_| Message::Tick),
            self.core()
                .watch_config::<Config>(Self::APP_ID)
                .map(|update| Message::UpdateConfig(update.config)),
        ])
    }

    fn update(&mut self, message: Self::Message) -> Task<cosmic::Action<Self::Message>> {
        match message {
            Message::UpdateConfig(config) => {
                self.config = config;
            }
            Message::Tick => {
                let now = Utc::now();
                let mut vendor_states = Vec::new();
                for v in &self.rules_config.vendors {
                    let status = check_vendor_pricing(v, now);
                    vendor_states.push(VendorState {
                        name: v.name.clone(),
                        icon: v.icon.clone(),
                        is_peak: status.is_peak,
                        rate: status.rate,
                        countdown_str: status.countdown_str,
                        next_change_time: status.next_change_time,
                        off_peak_reason: status.off_peak_reason,
                    });
                }
                self.vendor_states = vendor_states;
                self.last_update = now;

                let label_parts: Vec<String> = self.vendor_states.iter().map(|state| {
                    format!("{} {:.1}x • {}", state.icon, state.rate, state.countdown_str)
                }).collect();
                self.panel_label = label_parts.join(" | ");
            }
            Message::SetTabIndex(idx) => {
                self.current_tab_index = idx;
            }
            Message::TogglePopup => {
                return if let Some(p) = self.popup.take() {
                    destroy_popup(p)
                } else {
                    let new_id = Id::unique();
                    self.popup.replace(new_id);
                    let mut popup_settings = self.core.applet.get_popup_settings(
                        self.core.main_window_id().unwrap(),
                        new_id,
                        None,
                        None,
                        None,
                    );
                    popup_settings.positioner.size_limits = Limits::NONE
                        .max_width(370.0)
                        .min_width(300.0)
                        .min_height(200.0)
                        .max_height(600.0);
                    get_popup(popup_settings)
                }
            }
            Message::PopupClosed(id) => {
                if self.popup.as_ref() == Some(&id) {
                    self.popup = None;
                }
            }
        }
        Task::none()
    }

    fn style(&self) -> Option<cosmic::iced::theme::Style> {
        Some(cosmic::applet::style())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Parse the rules that ship in `resources/pricing_rules.json`, so these
    /// tests exercise the real configuration rather than a fixture that could
    /// drift away from it.
    fn rules() -> RulesConfig {
        serde_json::from_str(FALLBACK_RULES_JSON).expect("bundled pricing rules must parse")
    }

    fn vendor(name: &str) -> Vendor {
        rules()
            .vendors
            .into_iter()
            .find(|v| v.name == name)
            .unwrap_or_else(|| panic!("no {name} vendor in bundled rules"))
    }

    fn at(s: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
    }

    fn ds(s: &str) -> VendorStatus {
        check_vendor_pricing(&vendor("DeepSeek"), at(s))
    }

    fn glm(s: &str) -> VendorStatus {
        check_vendor_pricing(&vendor("GLM"), at(s))
    }

    // ---- DeepSeek: rule is Mon-Fri, 01:00-04:00 and 06:00-10:00 UTC ----

    #[test]
    fn deepseek_peak_on_a_weekday() {
        // 2026-09-28 is a Monday.
        let s = ds("2026-09-28T02:00:00Z");
        assert!(s.is_peak);
        assert_eq!(s.rate, 2.0);
        assert_eq!(s.off_peak_reason, OffPeakReason::Peak);
    }

    #[test]
    fn deepseek_weekend_is_off_peak_all_day() {
        // Regression: a Saturday peak window used to be billed at 2.0x even
        // though DeepSeek bills weekends as off-peak in full.
        let s = ds("2026-09-26T02:00:00Z");
        assert!(!s.is_peak, "Saturday 02:00 UTC must not be peak");
        assert_eq!(s.rate, 1.0);
        assert_eq!(s.off_peak_reason, OffPeakReason::Valley);

        let s = ds("2026-09-27T07:00:00Z"); // Sunday 06:00-10:00 window
        assert!(!s.is_peak, "Sunday 07:00 UTC must not be peak");
        assert_eq!(s.rate, 1.0);
    }

    #[test]
    fn deepseek_chinese_public_holiday_is_off_peak() {
        // National Day runs 2026-10-01..07 per the State Council notice, and
        // 2026-10-05 is the Monday inside it.
        let s = ds("2026-10-05T07:00:00Z");
        assert!(!s.is_peak, "a weekday holiday must not be billed as peak");
        assert_eq!(s.rate, 1.0);
        assert_eq!(s.off_peak_reason, OffPeakReason::Exception);
    }

    #[test]
    fn deepseek_holiday_range_boundaries_are_exact() {
        // Start is inclusive: first peak window of Oct 1 is already off-peak.
        assert!(!ds("2026-10-01T01:00:00Z").is_peak);
        // End is exclusive: Oct 8 is an ordinary Thursday, so peak resumes.
        let s = ds("2026-10-08T02:00:00Z");
        assert!(s.is_peak, "Oct 8 is outside the holiday window");
        assert_eq!(s.rate, 2.0);
    }

    #[test]
    fn deepseek_next_change_skips_the_weekend() {
        // From a Saturday, the next peak transition is Monday 01:00 UTC, not
        // Sunday's ignored window.
        let s = ds("2026-09-26T02:00:00Z");
        assert_eq!(s.next_change_time, at("2026-09-28T01:00:00Z"));
    }

    #[test]
    fn deepseek_next_change_ends_a_weekday_peak() {
        let s = ds("2026-09-28T02:00:00Z");
        assert_eq!(s.next_change_time, at("2026-09-28T04:00:00Z"));
    }

    #[test]
    fn deepseek_friday_peak_exits_to_monday() {
        // 2026-10-09 is a Friday; its 06:00-10:00 UTC peak ends at 10:00 the
        // same day, and the next peak start is Monday Oct 12 because Oct 10-11
        // are a weekend.
        let peak = ds("2026-10-09T07:00:00Z");
        assert!(peak.is_peak);
        assert_eq!(peak.next_change_time, at("2026-10-09T10:00:00Z"));

        let after = ds("2026-10-09T11:00:00Z");
        assert!(!after.is_peak);
        assert_eq!(after.next_change_time, at("2026-10-12T01:00:00Z"));
    }

    // ---- GLM: rule is Mon-Fri, 14:00-18:00 UTC+8 (= 06:00-10:00 UTC) ----

    #[test]
    fn glm_peak_on_a_weekday_uses_singapore_time() {
        // 2026-10-14 is a Wednesday; 08:00 UTC == 16:00 SGT, inside 14-18.
        let s = glm("2026-10-14T08:00:00Z");
        assert!(s.is_peak);
        assert_eq!(s.rate, 3.0);
    }

    #[test]
    fn glm_outside_peak_window_is_off_peak() {
        // 00:00 UTC == 08:00 SGT, before the window, after the promo ended.
        let s = glm("2026-10-14T00:00:00Z");
        assert!(!s.is_peak);
        assert_eq!(s.rate, 2.0);
        assert_eq!(s.next_change_time, at("2026-10-14T06:00:00Z"));
    }

    #[test]
    fn glm_promotion_applies_while_live() {
        // 2026-09-23 is the Wednesday before the Mid-Autumn block, so the
        // limited-time 1.0x off-peak benefit is still running.
        let valley = glm("2026-09-23T00:00:00Z");
        assert!(!valley.is_peak);
        assert_eq!(valley.rate, 1.0);
        assert_eq!(valley.next_change_time, at("2026-09-23T06:00:00Z"));

        let peak = glm("2026-09-23T08:00:00Z");
        assert!(peak.is_peak);
        assert_eq!(peak.rate, 3.0);
    }

    #[test]
    fn glm_weekend_is_off_peak_all_day() {
        // Regression: 06:00-10:00 UTC on a Saturday used to be billed at 3.0x.
        let s = glm("2026-09-26T07:00:00Z");
        assert!(!s.is_peak, "Saturday must not be peak for GLM");
        assert_eq!(s.rate, 1.0);
    }

    #[test]
    fn glm_holiday_grace_period_is_off_peak() {
        // z.ai bills 2026-09-25..10-07 as off-peak all day; Oct 5 is a Monday.
        let s = glm("2026-10-05T07:00:00Z");
        assert!(!s.is_peak);
        assert_eq!(s.rate, 1.0);
        assert_eq!(s.off_peak_reason, OffPeakReason::Exception);
    }

    #[test]
    fn glm_grace_period_ends_when_the_promotion_does() {
        // The exception window and promotion end at the same instant:
        // 2026-10-07T16:00:00Z == 2026-10-08T00:00:00+08:00, so the final
        // second of the window still bills 1.0x.
        let last = glm("2026-10-07T15:59:59Z");
        assert!(!last.is_peak, "still inside the all-day off-peak window");
        assert_eq!(last.rate, 1.0);
        assert_eq!(last.off_peak_reason, OffPeakReason::Exception);

        // An SGT Thursday after the window: peak returns at 3.0x, and the
        // off-peak rate returns to the standing 2.0x.
        let peak = glm("2026-10-08T06:00:00Z");
        assert!(peak.is_peak);
        assert_eq!(peak.rate, 3.0);

        let valley = glm("2026-10-08T00:00:00Z");
        assert!(!valley.is_peak);
        assert_eq!(valley.rate, 2.0);
        assert_eq!(valley.off_peak_reason, OffPeakReason::Valley);
    }

    // ---- invariants across a long sweep ----

    #[test]
    fn next_change_is_always_ahead_and_near() {
        let start = at("2026-09-24T00:00:00Z");
        for v in rules().vendors {
            for step in 0..(24 * 30) {
                let now = start + Duration::hours(step);
                let s = check_vendor_pricing(&v, now);
                assert!(
                    s.next_change_time > now,
                    "{} reported a past transition {} at {}",
                    v.name,
                    s.next_change_time,
                    now
                );
                assert!(
                    s.next_change_time - now <= Duration::days(TRANSITION_LOOKAHEAD_DAYS),
                    "{} reported an implausibly distant transition from {}",
                    v.name,
                    now
                );
                assert!(
                    !s.countdown_str.is_empty(),
                    "{} produced an empty countdown at {}",
                    v.name,
                    now
                );
                // A peak can never be reported while the rate is off-peak-only.
                if s.is_peak {
                    assert_eq!(s.off_peak_reason, OffPeakReason::Peak);
                    assert!(
                        s.rate > off_peak_rate(&v, now),
                        "{} peak rate {} must exceed its off-peak rate at {}",
                        v.name,
                        s.rate,
                        now
                    );
                }
            }
        }
    }

    #[test]
    fn bundled_rules_are_well_formed() {
        let rules = rules();
        assert!(!rules.vendors.is_empty());
        for v in &rules.vendors {
            assert!(!v.name.is_empty() && !v.icon.is_empty());
            assert!(!v.peaks.is_empty(), "{} has no peak windows", v.name);
            if let Some(days) = &v.days_of_week {
                assert!(!days.is_empty(), "{} has an empty days_of_week", v.name);
                for d in days {
                    assert!((1..=7).contains(d), "{} has weekday {d} outside 1..=7", v.name);
                }
            }
            for p in &v.peaks {
                assert!(
                    p.start_hour < p.end_hour && p.end_hour <= 24,
                    "{} has malformed window {}-{}",
                    v.name,
                    p.start_hour,
                    p.end_hour
                );
                assert!(p.rate > v.default_rate, "{} peak must exceed off-peak", v.name);
            }
            for e in &v.exceptions {
                assert!(e.end > e.start, "{} has an empty exception range", v.name);
            }
        }
    }

    #[test]
    fn exception_deserializer_accepts_both_shapes() {
        let json = r#"{
            "name": "T", "icon": "x", "timezone_offset_hours": 0,
            "activation_date": null, "default_rate": 1.0,
            "peaks": [{"start_hour": 1, "end_hour": 2, "rate": 2.0}],
            "exceptions": ["2026-01-01", ["2026-02-01", "2026-02-04"]],
            "promotion": null
        }"#;
        let v: Vendor = serde_json::from_str(json).unwrap();
        assert_eq!(v.exceptions.len(), 2);
        assert!(v.in_exception(NaiveDate::from_ymd_opt(2026, 1, 1).unwrap()));
        assert!(!v.in_exception(NaiveDate::from_ymd_opt(2026, 1, 2).unwrap()));
        assert!(v.in_exception(NaiveDate::from_ymd_opt(2026, 2, 1).unwrap()));
        assert!(v.in_exception(NaiveDate::from_ymd_opt(2026, 2, 3).unwrap()));
        assert!(!v.in_exception(NaiveDate::from_ymd_opt(2026, 2, 4).unwrap()));
    }

    #[test]
    fn rule_fields_are_optional_for_older_fetched_rules() {
        // A rules file fetched before this change has neither key; it must still
        // load, falling back to "every day" with no exceptions.
        let json = r#"{
            "name": "Legacy", "icon": "x", "timezone_offset_hours": 0,
            "activation_date": null, "default_rate": 1.0,
            "peaks": [{"start_hour": 1, "end_hour": 4, "rate": 2.0}],
            "promotion": null
        }"#;
        let v: Vendor = serde_json::from_str(json).unwrap();
        assert!(v.days_of_week.is_none());
        assert!(v.exceptions.is_empty());
        // 2026-09-26 is a Saturday, but with no day rule it is peak as before.
        let s = check_vendor_pricing(&v, at("2026-09-26T02:00:00Z"));
        assert!(s.is_peak);
    }

    #[test]
    fn malformed_exception_ranges_are_rejected() {
        let bad = r#"{
            "name": "T", "icon": "x", "timezone_offset_hours": 0,
            "activation_date": null, "default_rate": 1.0,
            "peaks": [], "exceptions": [["2026-02-04", "2026-02-01"]],
            "promotion": null
        }"#;
        assert!(serde_json::from_str::<Vendor>(bad).is_err());
    }
}
