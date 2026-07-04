// SPDX-License-Identifier: MIT

use crate::config::Config;
use cosmic::cosmic_config::{self, CosmicConfigEntry};
use cosmic::iced::platform_specific::shell::wayland::commands::popup::{destroy_popup, get_popup};
use cosmic::iced::{window::Id, Limits, Subscription};
use cosmic::prelude::*;
use cosmic::widget;
use chrono::{DateTime, Utc, Timelike, Duration};
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

#[derive(Debug, Serialize, Deserialize, Clone)]
struct Vendor {
    name: String,
    icon: String,
    timezone_offset_hours: i32,
    activation_date: Option<String>,
    default_rate: f32,
    peaks: Vec<Peak>,
    promotion: Option<Promotion>,
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
}

fn load_rules() -> RulesConfig {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/home/ew".to_string());
    let cache_dir = std::path::PathBuf::from(home).join(".config").join("tkmon");
    let cache_file = cache_dir.join("pricing_rules.json");

    let default_url = "https://raw.githubusercontent.com/user/tkmon/main/pricing_rules.json";
    
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

fn check_vendor_pricing(vendor: &Vendor, now: DateTime<Utc>) -> (bool, f32, DateTime<Utc>, String) {
    if let Some(ref act_str) = vendor.activation_date {
        if let Ok(act_date) = DateTime::parse_from_rfc3339(act_str) {
            let act_utc = act_date.with_timezone(&Utc);
            if now < act_utc {
                return (false, vendor.default_rate, act_utc, format_duration(act_utc - now));
            }
        }
    }

    let offset = Duration::hours(vendor.timezone_offset_hours as i64);
    let now_local = now + offset;
    let hour = now_local.hour();

    let mut current_peak: Option<&Peak> = None;
    for peak in &vendor.peaks {
        if peak.start_hour <= hour && hour < peak.end_hour {
            current_peak = Some(peak);
            break;
        }
    }

    let is_peak = current_peak.is_some();
    let rate = if let Some(peak) = current_peak {
        peak.rate
    } else {
        let mut off_peak_rate = vendor.default_rate;
        if let Some(ref promo) = vendor.promotion {
            if let Ok(promo_end) = DateTime::parse_from_rfc3339(&promo.end_date) {
                if now < promo_end.with_timezone(&Utc) {
                    off_peak_rate = promo.off_peak_rate;
                }
            }
        }
        off_peak_rate
    };

    let base_today_local = now_local.with_minute(0).unwrap().with_second(0).unwrap().with_nanosecond(0).unwrap();
    
    let mut transition_hours = std::collections::BTreeSet::new();
    for peak in &vendor.peaks {
        transition_hours.insert(peak.start_hour);
        transition_hours.insert(peak.end_hour);
    }

    if transition_hours.is_empty() {
        let next_hour = now + Duration::hours(1);
        return (false, rate, next_hour, format_duration(next_hour - now));
    }

    let mut next_change_utc = None;
    for day_offset in 0..2 {
        let base_day_local = base_today_local + Duration::days(day_offset);
        for &h in &transition_hours {
            if let Some(trans_local) = base_day_local.with_hour(h) {
                let trans_utc = trans_local - offset;
                if trans_utc > now {
                    if next_change_utc.is_none() || trans_utc < next_change_utc.unwrap() {
                        next_change_utc = Some(trans_utc);
                    }
                }
            }
        }
    }

    let next_change = next_change_utc.unwrap_or_else(|| now + Duration::hours(1));
    (is_peak, rate, next_change, format_duration(next_change - now))
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
            let (is_peak, rate, next_change_time, countdown_str) = check_vendor_pricing(v, now);
            vendor_states.push(VendorState {
                name: v.name.clone(),
                icon: v.icon.clone(),
                is_peak,
                rate,
                countdown_str,
                next_change_time,
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
            let status_str = if state.is_peak {
                format!("📈 PEAK HOUR ({:.1}x price)", state.rate)
            } else {
                format!("📉 VALLEY HOUR ({:.1}x price)", state.rate)
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
                    let (is_peak, rate, next_change_time, countdown_str) = check_vendor_pricing(v, now);
                    vendor_states.push(VendorState {
                        name: v.name.clone(),
                        icon: v.icon.clone(),
                        is_peak,
                        rate,
                        countdown_str,
                        next_change_time,
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
