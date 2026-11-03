//! What the phone has to allow for alarms to ring on time, how to change it, and the
//! one check that holds on any phone: a real alarm. Also the first-run setup, which is
//! this list with a word of welcome.

use dioxus::prelude::*;
use mobile_sentinel::health::{self, Check, HealthReport};

use crate::app::{t, AlarmData, Route, Translations};
use crate::back::use_back_handler;
use crate::components::{Icon, IconName, Row, ScreenHeader, ToastMessage};
use crate::models::AppSettings;

/// A test alarm rings about this long from now.
const TEST_SECONDS: i64 = 60;

/// The checks in the order the screen lists them.
const CHECKS: [Check; 10] = [
    Check::ExactAlarms,
    Check::Notifications,
    Check::FullScreen,
    Check::Overlay,
    Check::BatteryUnrestricted,
    Check::BackgroundAllowed,
    Check::KeptWhenUnused,
    Check::ChannelImportance,
    Check::NotTotalSilence,
    Check::BootSignal,
];

fn report() -> HealthReport {
    health::report(crate::platform::FOREGROUND_CHANNEL_ID)
}

/// The most serious thing standing in an alarm's way, for the banner on Home, leaving out
/// what the user hid there.
pub fn first_problem(hidden: &[String]) -> Option<Check> {
    report()
        .problems()
        .into_iter()
        .find(|check| !can_hide(*check) || !hidden.iter().any(|name| name == check_name(*check)))
}

/// Whether the user may stop seeing a warning on Home. Not the ones that keep an alarm from
/// ringing or from being seen.
pub fn can_hide(check: Check) -> bool {
    matches!(
        check,
        Check::Overlay | Check::BatteryUnrestricted | Check::KeptWhenUnused | Check::BootSignal
    )
}

/// What a check goes by, in the translations and the settings.
pub fn check_name(check: Check) -> &'static str {
    match check {
        Check::ExactAlarms => "exact_alarms",
        Check::Notifications => "notifications",
        Check::FullScreen => "full_screen",
        Check::Overlay => "overlay",
        Check::BatteryUnrestricted => "battery",
        Check::BackgroundAllowed => "background",
        Check::KeptWhenUnused => "unused",
        Check::ChannelImportance => "channel",
        Check::NotTotalSilence => "silence",
        Check::BootSignal => "boot_signal",
    }
}

/// What a check is called, and what it means when it is not fine.
pub fn check_text(translations: &Translations, check: Check) -> (String, String) {
    let name = check_name(check);
    (
        t(translations, &format!("reliability.{name}")),
        t(translations, &format!("reliability.{name}_note")),
    )
}

/// The moment and speed of the last ring, from the SDK's record: when it was delivered,
/// and how many ms later the sound started.
fn last_ring(log: &str) -> Option<(i64, i64)> {
    let mut delivered = None;
    let mut result = None;
    for line in log.lines() {
        let mut parts = line.split_whitespace();
        let (Some(at), Some(event)) = (parts.next(), parts.next()) else {
            continue;
        };
        let Ok(at) = at.parse::<i64>() else {
            continue;
        };
        match event {
            "delivered" => delivered = Some(at),
            "sound" => {
                if let Some(start) = delivered.take() {
                    result = Some((start, at - start));
                }
            }
            _ => {}
        }
    }
    result
}

/// The reliability screen, also as the first-run setup.
#[component]
pub fn Reliability() -> Element {
    rsx! {
        ReliabilityScreen { first_run: false }
    }
}

/// Shown once, before anything else.
#[component]
pub fn FirstRun() -> Element {
    rsx! {
        ReliabilityScreen { first_run: true }
    }
}

#[component]
fn ReliabilityScreen(first_run: bool) -> Element {
    let translations = use_context::<Translations>();
    let mut alarms = use_context::<Signal<Vec<AlarmData>>>();
    let mut settings = use_context::<Signal<AppSettings>>();
    let mut toast = use_context::<Signal<Option<ToastMessage>>>();
    let nav = use_navigator();

    // The user changes these on the system's pages and comes back.
    let mut current = use_signal(report);
    use_future(move || async move {
        loop {
            crate::timer::sleep_ms(1500).await;
            let fresh = report();
            if fresh != *current.peek() {
                current.set(fresh);
            }
        }
    });

    let mut leave = move || {
        if first_run {
            settings.write().prefs.setup_done = true;
            crate::persistence::save_settings(&settings.peek());
            nav.replace(Route::Home {});
        } else {
            nav.go_back();
        }
    };
    use_back_handler(EventHandler::new(move |_| leave()));

    let health = current();
    let last_test = crate::platform::alarm_kit_optional()
        .and_then(|kit| last_ring(&kit.timing_log()))
        .map(|(_, took_ms)| {
            t(&translations, "reliability.last_ring")
                .replace("{seconds}", &format!("{:.1}", took_ms as f64 / 1000.0))
        })
        .unwrap_or_default();
    let has_ring_record =
        crate::platform::alarm_kit_optional().is_some_and(|kit| !kit.timing_log().is_empty());

    rsx! {
        div { class: "screen",
            if first_run {
                header { class: "setup-header",
                    h1 { class: "screen-title-large", {t(&translations, "setup.title")} }
                }
            } else {
                ScreenHeader {
                    title: t(&translations, "reliability.title"),
                    on_back: move |_| leave(),
                }
            }
            div { class: "scroll",
                p { class: "intro",
                    if first_run {
                        {t(&translations, "setup.intro")}
                    } else {
                        {t(&translations, "reliability.intro")}
                    }
                }
                ul { class: "list",
                    for check in CHECKS {
                        {
                            let ok = health.is_ok(check);
                            let (title, note) = check_text(&translations, check);
                            let status = if ok {
                                t(&translations, "reliability.ok")
                            } else {
                                t(&translations, "reliability.attention")
                            };
                            rsx! {
                                li { class: "check-row",
                                    span {
                                        class: if ok { "check-icon ok" } else { "check-icon warn" },
                                        role: "img",
                                        "aria-label": "{status}",
                                        Icon { name: if ok { IconName::Check } else { IconName::Warning }, size: 20 }
                                    }
                                    span { class: "check-text",
                                        span { class: "check-title", "{title}" }
                                        if !ok {
                                            span { class: "check-note", "{note}" }
                                        }
                                    }
                                    // No app can open the maker's page for this one.
                                    if !ok && check != Check::BootSignal {
                                        button {
                                            class: "btn-small",
                                            onclick: move |_| {
                                                // Notifications are asked for in place first, which only
                                                // works while Android has not been told no.
                                                if check == Check::Notifications {
                                                    let _ = mobile_sentinel::permissions::request("android.permission.POST_NOTIFICATIONS");
                                                    return;
                                                }
                                                // One tap in Android's own dialog, instead of a list
                                                // of every app on the phone.
                                                if check == Check::BatteryUnrestricted {
                                                    mobile_sentinel::battery::request_exemption();
                                                    return;
                                                }
                                                health::open_settings(check, crate::platform::FOREGROUND_CHANNEL_ID);
                                            },
                                            {t(&translations, "reliability.fix")}
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                ul { class: "list",
                    Row {
                        title: t(&translations, "reliability.app_settings"),
                        on_click: move |_| mobile_sentinel::permissions::open_app_settings(),
                    }
                    if has_ring_record {
                        Row {
                            title: t(&translations, "reliability.save_record"),
                            on_click: move |_| {
                                let record = crate::platform::alarm_kit_optional()
                                    .map(|kit| kit.timing_log())
                                    .unwrap_or_default();
                                let saved = crate::platform::save_to_downloads(
                                    &record,
                                    "alarmfree-rings.txt",
                                    "text/plain",
                                );
                                toast.set(Some(match saved {
                                    Ok(place) => ToastMessage::info(
                                        t(&translations, "settings.backup_saved").replace("{place}", &place),
                                    ),
                                    Err(e) => {
                                        log::warn!("[AlarmFree] The ring record was not saved: {e}");
                                        ToastMessage::error(t(&translations, "settings.backup_failed"))
                                    }
                                }));
                            },
                        }
                    }
                }
            }
            div { class: "save-bar save-bar-column",
                if !first_run {
                    p { class: "test-note",
                        {t(&translations, "reliability.test_note")}
                        " {last_test}"
                    }
                    button {
                        class: "btn btn-primary",
                        onclick: move |_| {
                            let now = chrono::Local::now().naive_local();
                            let label = t(&translations, "reliability.test_label");
                            let alarm = crate::alarm_ops::test_alarm(now, TEST_SECONDS, label);
                            alarms.write().push(alarm);
                            crate::persistence::save_alarms(&alarms.peek());
                            toast.set(Some(ToastMessage::info(t(&translations, "reliability.test_set"))));
                        },
                        {t(&translations, "reliability.test")}
                    }
                } else {
                    button {
                        class: "btn btn-primary",
                        onclick: move |_| leave(),
                        {t(&translations, "setup.done")}
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::last_ring;

    #[test]
    fn the_last_complete_ring_counts() {
        let log =
            "1000 delivered a\n1100 service a\n1400 sound a\n5000 delivered b\n5300 sound b\n";
        assert_eq!(last_ring(log), Some((5000, 300)));
    }

    #[test]
    fn a_ring_without_sound_says_nothing_about_speed() {
        assert_eq!(last_ring("1000 delivered a\n1100 service a\n"), None);
        assert_eq!(last_ring(""), None);
        assert_eq!(
            last_ring("1000 delivered a\n1400 sound a\n2000 delivered b\n"),
            Some((1000, 400))
        );
    }
}
