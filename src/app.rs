// SPDX-License-Identifier: MIT

use crate::config::Config;
use cosmic::cosmic_config::{self, CosmicConfigEntry};
use cosmic::iced::platform_specific::shell::wayland::commands::popup::{destroy_popup, get_popup};
use cosmic::iced::{window::Id, Limits, Subscription};
use cosmic::prelude::*;
use cosmic::widget;
use chrono::{DateTime, Utc, TimeZone, Timelike, Duration};
use std::sync::LazyLock;

static AUTOSIZE_MAIN_ID: LazyLock<cosmic::widget::Id> = LazyLock::new(|| cosmic::widget::Id::new("cosmic-applet-autosize-main"));

fn check_pricing_status(now: DateTime<Utc>) -> (bool, DateTime<Utc>, String) {
    let policy_start = Utc.with_ymd_and_hms(2026, 7, 15, 0, 0, 0).unwrap();
    
    if now < policy_start {
        // First peak starts at 1:00 AM UTC on July 15th
        let first_peak = Utc.with_ymd_and_hms(2026, 7, 15, 1, 0, 0).unwrap();
        return (false, first_peak, format_duration(first_peak - now));
    }
    
    let hour = now.hour();
    let is_peak = (1 <= hour && hour < 4) || (6 <= hour && hour < 10);
    
    let base_today = now.with_minute(0).unwrap().with_second(0).unwrap().with_nanosecond(0).unwrap();
    let next_change = if 1 <= hour && hour < 4 {
        base_today.with_hour(4).unwrap()
    } else if 4 <= hour && hour < 6 {
        base_today.with_hour(6).unwrap()
    } else if 6 <= hour && hour < 10 {
        base_today.with_hour(10).unwrap()
    } else if hour < 1 {
        base_today.with_hour(1).unwrap()
    } else {
        let tomorrow = base_today + Duration::days(1);
        tomorrow.with_hour(1).unwrap()
    };
    
    (is_peak, next_change, format_duration(next_change - now))
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

/// The application model stores app-specific state used to describe its interface and
/// drive its logic.
pub struct AppModel {
    /// Application state which is managed by the COSMIC runtime.
    core: cosmic::Core,
    /// The popup id.
    popup: Option<Id>,
    /// Configuration data that persists between application runs.
    config: Config,
    /// Is currently peak hours
    is_peak: bool,
    /// Formatted countdown string
    countdown_str: String,
    /// Time of the next switch
    next_change_time: DateTime<Utc>,
    /// Last update time
    last_update: DateTime<Utc>,
    /// Label displayed in the panel
    panel_label: String,
}

/// Messages emitted by the application and its widgets.
#[derive(Debug, Clone)]
pub enum Message {
    TogglePopup,
    PopupClosed(Id),
    UpdateConfig(Config),
    Tick,
}

/// Create a COSMIC application from the app model
impl cosmic::Application for AppModel {
    /// The async executor that will be used to run your application's commands.
    type Executor = cosmic::executor::Default;

    /// Data that your application receives to its init method.
    type Flags = ();

    /// Messages which the application and its widgets will emit.
    type Message = Message;

    /// Unique identifier in RDNN (reverse domain name notation) format.
    const APP_ID: &'static str = "com.system76.CosmicAppletDeepseek";

    fn core(&self) -> &cosmic::Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut cosmic::Core {
        &mut self.core
    }

    /// Initializes the application with any given flags and startup commands.
    fn init(
        core: cosmic::Core,
        _flags: Self::Flags,
    ) -> (Self, Task<cosmic::Action<Self::Message>>) {
        // Construct the app model with the runtime's core.
        let config = cosmic_config::Config::new(Self::APP_ID, Config::VERSION)
            .map(|context| match Config::get_entry(&context) {
                Ok(config) => config,
                Err((_errors, config)) => config,
            })
            .unwrap_or_default();

        let now = Utc::now();
        let (is_peak, next_change_time, countdown_str) = check_pricing_status(now);
        let status_text = if is_peak { "📈 PEAK (2x)" } else { "📉 VALLEY (1x)" };
        let panel_label = format!("{} • {}", status_text, countdown_str);

        let app = AppModel {
            core,
            popup: None,
            config,
            is_peak,
            countdown_str,
            next_change_time,
            last_update: now,
            panel_label,
        };

        (app, Task::none())
    }

    fn on_close_requested(&self, id: Id) -> Option<Message> {
        Some(Message::PopupClosed(id))
    }

    /// Describes the interface based on the current state of the application model.
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

        eprintln!("DEBUG DEEPSEEK VIEW: panel_label = {}, width = {:?}", self.panel_label, self.panel_label.len());

        cosmic::widget::autosize::autosize(button, AUTOSIZE_MAIN_ID.clone()).into()
    }

    /// The applet's popup window will be drawn using this view method.
    fn view_window(&self, _id: Id) -> Element<'_, Self::Message> {
        let now_local = self.last_update.with_timezone(&chrono::Local);
        let switch_local = self.next_change_time.with_timezone(&chrono::Local);

        let status_str = if self.is_peak { "📈 PEAK HOUR (2x price)" } else { "📉 VALLEY HOUR (1x price)" };
        let switch_state = if self.is_peak { "VALLEY HOUR" } else { "PEAK HOUR" };

        let content_list = widget::list_column()
            .add(widget::settings::item(
                "Pricing Status".to_string(),
                widget::text(status_str).size(14),
            ))
            .add(widget::settings::item(
                "Next Switch to".to_string(),
                widget::text(switch_state).size(14),
            ))
            .add(widget::settings::item(
                "Time Remaining".to_string(),
                widget::text(&self.countdown_str).size(14),
            ))
            .add(widget::settings::item(
                "Switch Time (Local)".to_string(),
                widget::text(switch_local.format("%Y-%m-%d %H:%M:%S %Z").to_string()).size(12),
            ))
            .add(widget::settings::item(
                "Switch Time (UTC)".to_string(),
                widget::text(self.next_change_time.format("%Y-%m-%d %H:%M:%S UTC").to_string()).size(12),
            ))
            .add(widget::settings::item(
                "Last Updated".to_string(),
                widget::text(now_local.format("%H:%M:%S %Z").to_string()).size(12),
            ));

        self.core.applet.popup_container(content_list).into()
    }

    /// Register subscriptions for this application.
    fn subscription(&self) -> Subscription<Self::Message> {
        Subscription::batch(vec![
            // Tick every 10 seconds to update pricing status and countdown
            cosmic::iced::time::every(std::time::Duration::from_secs(10))
                .map(|_| Message::Tick),
            
            // Watch for application configuration changes.
            self.core()
                .watch_config::<Config>(Self::APP_ID)
                .map(|update| Message::UpdateConfig(update.config)),
        ])
    }

    /// Handles messages emitted by the application and its widgets.
    fn update(&mut self, message: Self::Message) -> Task<cosmic::Action<Self::Message>> {
        match message {
            Message::UpdateConfig(config) => {
                self.config = config;
            }
            Message::Tick => {
                let now = Utc::now();
                let (is_peak, next_change_time, countdown_str) = check_pricing_status(now);
                self.is_peak = is_peak;
                self.next_change_time = next_change_time;
                self.countdown_str = countdown_str;
                self.last_update = now;
                let status_text = if is_peak { "📈 PEAK (2x)" } else { "📉 VALLEY (1x)" };
                self.panel_label = format!("{} • {}", status_text, self.countdown_str);
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
