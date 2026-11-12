//! What the phone has to allow for alarms to ring on time: which checks there are, what they
//! are called, which warnings the user may hide, and how fast the last ring came.

use mobile_sentinel::health::{self, Check, HealthReport};

use crate::i18n::{t, Translations};

/// The checks in the order the screen lists them.
pub const CHECKS: [Check; 10] = [
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

pub fn report() -> HealthReport {
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
pub fn last_ring(log: &str) -> Option<(i64, i64)> {
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

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn only_the_harmless_warnings_can_be_hidden() {
        for check in CHECKS {
            let hidable = matches!(
                check,
                Check::Overlay
                    | Check::BatteryUnrestricted
                    | Check::KeptWhenUnused
                    | Check::BootSignal
            );
            assert_eq!(can_hide(check), hidable);
        }
    }

    #[test]
    fn every_check_has_a_name_and_a_text() {
        let names: std::collections::HashSet<_> = CHECKS.iter().map(|c| check_name(*c)).collect();
        assert_eq!(names.len(), CHECKS.len());
        let tr = crate::i18n::load_translations(crate::models::Locale::default());
        for check in CHECKS {
            let (title, note) = check_text(&tr, check);
            assert!(!title.starts_with("reliability."), "{title}");
            assert!(!note.starts_with("reliability."), "{note}");
        }
    }
}
