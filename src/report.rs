//! The bug report: what the user wrote, what the phone is, and the diagnostic log, as one text
//! the user reads before sending it with their own email app. Nothing here sends anything.

use crate::models::AlarmData;

/// Where reports go: the public developer address from the Play listing. If it is ever empty the
/// email app opens with no recipient and the user can type one.
pub const CONTACT_EMAIL: &str = "r.fidder97@gmail.com";

/// The Android versionCode of this build, as in `sentinel.toml`.
pub const VERSION_CODE: u32 = 12401;

/// The file the report is attached as.
pub const REPORT_FILE: &str = "alarmfree-report.txt";

/// The alarm backup, attached when the user asks for it.
pub const SETUP_FILE: &str = "alarmfree-setup.json";

/// How many lines of the log go into a report.
pub const LOG_LINES: usize = 600;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Kind {
    #[default]
    Problem,
    Idea,
}

impl Kind {
    pub fn name(self) -> &'static str {
        match self {
            Kind::Problem => "Problem",
            Kind::Idea => "Idea",
        }
    }
}

/// What the user typed.
#[derive(Debug, Clone, Default)]
pub struct Answers {
    pub kind: Kind,
    pub what: String,
    pub expected: String,
    pub when: String,
}

/// What a report is made of besides the answers.
pub struct Parts<'a> {
    pub device: &'a str,
    pub permissions: &'a str,
    pub alarms: &'a [AlarmData],
    pub log: &'a str,
    pub include_log: bool,
    pub backup_attached: bool,
    pub created: &'a str,
}

/// Replace the label and the note of every alarm wherever they occur: a safety net on top of
/// never writing them to the log. Texts shorter than three characters would hit everything.
pub fn scrub(text: &str, alarms: &[AlarmData]) -> String {
    let mut secrets: Vec<&str> = alarms
        .iter()
        .flat_map(|a| [a.label.trim(), a.note.trim()])
        .filter(|s| s.chars().count() >= 3)
        .collect();
    secrets.sort_by_key(|s| std::cmp::Reverse(s.len()));
    secrets.dedup();
    let mut out = text.to_owned();
    for secret in secrets {
        out = out.replace(secret, "[text]");
    }
    out
}

/// The subject: `AlarmFree 0.5.1 (12401): <first words>`.
pub fn subject(kind: Kind, version: &str, code: u32, what: &str) -> String {
    let words: Vec<&str> = what.split_whitespace().take(7).collect();
    let mut head = words.join(" ");
    if head.chars().count() > 50 {
        head = head.chars().take(50).collect::<String>() + "...";
    }
    let tail = match (kind, head.is_empty()) {
        (Kind::Idea, true) => "Idea".to_owned(),
        (Kind::Idea, false) => format!("Idea: {head}"),
        (Kind::Problem, true) => "Problem".to_owned(),
        (Kind::Problem, false) => head,
    };
    format!("AlarmFree {version} ({code}): {tail}")
}

/// The answers and the app, which is also what the email body says.
pub fn summary(answers: &Answers, version: &str, created: &str) -> String {
    let or_dash = |s: &str| {
        if s.trim().is_empty() {
            "-".to_owned()
        } else {
            s.trim().to_owned()
        }
    };
    format!(
        "AlarmFree report\nType: {}\nApp: AlarmFree {version} ({VERSION_CODE})\nCreated: {created}\n\nWhat happened:\n{}\n\nWhat did you expect:\n{}\n\nRoughly when: {}\n",
        answers.kind.name(),
        or_dash(&answers.what),
        or_dash(&answers.expected),
        or_dash(&answers.when),
    )
}

/// The whole report: what is attached as `alarmfree-report.txt` and shown under "Show what is sent".
pub fn build(answers: &Answers, version: &str, parts: &Parts) -> String {
    let mut text = summary(answers, version, parts.created);
    text.push_str("\n== Device ==\n");
    text.push_str(parts.device.trim_end());
    text.push_str("\n\n== Permissions ==\n");
    text.push_str(parts.permissions.trim_end());
    let listed: Vec<&AlarmData> = parts.alarms.iter().filter(|a| !a.test).collect();
    text.push_str(&format!("\n\n== Alarms ({}) ==\n", listed.len()));
    text.push_str("(times and repeat only; no names or notes)\n");
    for alarm in listed {
        text.push_str(&crate::diag_log::alarm_line(alarm));
        text.push('\n');
    }
    text.push_str(&format!(
        "\n== Alarm setup file ==\n{}\n",
        if parts.backup_attached {
            "attached (alarmfree-setup.json)"
        } else {
            "not attached"
        }
    ));
    text.push_str("\n== Log ==\n");
    if parts.include_log {
        let tail = mobile_sentinel::diag::tail(parts.log, LOG_LINES);
        text.push_str(&scrub(&tail, parts.alarms));
    } else {
        text.push_str("not attached\n");
    }
    text
}

/// The date and time of the last ring that started, from the log, as `2026-11-14 07:30`.
pub fn last_ring_time(log: &str) -> Option<String> {
    log.lines()
        .rev()
        .find(|line| line.split(' ').nth(1) == Some("ring") && line.contains(" started "))
        .and_then(|line| {
            let stamp = line.split(' ').next()?;
            Some(stamp.get(..16)?.replace('T', " "))
        })
}

/// Whether the log has a panic or a missed alarm after `since` (a log timestamp; empty for
/// from the start): what the banner "Something went wrong last time" is about.
pub fn trouble_since(log: &str, since: &str) -> bool {
    log.lines()
        .filter(|line| since.is_empty() || line.get(..since.len()).is_some_and(|t| t > since))
        .any(|line| matches!(line.split(' ').nth(1), Some("panic" | "missed")))
}

/// The timestamp of the last line of the log, to remember as seen.
pub fn newest_stamp(log: &str) -> String {
    log.lines()
        .last()
        .and_then(|line| line.split(' ').next())
        .unwrap_or_default()
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn alarm(label: &str, note: &str) -> AlarmData {
        let mut a = crate::alarm_ops::factory_alarm();
        a.id = "aaaaaaaa-bbbb".into();
        a.label = label.into();
        a.note = note.into();
        a
    }

    fn parts<'a>(alarms: &'a [AlarmData], log: &'a str) -> Parts<'a> {
        Parts {
            device: "Device: Test Phone\nTime zone: Europe/Amsterdam (UTC+01:00)",
            permissions: "exact_alarms: ok",
            alarms,
            log,
            include_log: true,
            backup_attached: false,
            created: "2026-11-14T10:00:00+01:00",
        }
    }

    #[test]
    fn the_subject_has_version_code_and_the_first_words() {
        assert_eq!(
            subject(
                Kind::Problem,
                "0.5.0",
                12400,
                "The alarm did not ring at all this morning, sorry"
            ),
            "AlarmFree 0.5.0 (12400): The alarm did not ring at all"
        );
        assert_eq!(
            subject(Kind::Idea, "0.5.0", 12400, "  "),
            "AlarmFree 0.5.0 (12400): Idea"
        );
        assert!(subject(Kind::Idea, "0.5.0", 12400, "a b").ends_with("Idea: a b"));
        assert!(subject(Kind::Problem, "0.5.0", 1, &"x".repeat(200)).len() < 90);
    }

    #[test]
    fn the_scrub_removes_labels_and_notes_from_any_text() {
        let alarms = [
            alarm("Secret errand", "pills in the blue box"),
            alarm("ab", ""),
        ];
        let log = "T alarm saved Secret errand at 07:00\nT ring note pills in the blue box ab\n";
        let clean = scrub(log, &alarms);
        assert!(!clean.contains("Secret errand"));
        assert!(!clean.contains("pills"));
        assert!(clean.contains("[text]"));
        // Two letters would cut up ordinary words: left alone.
        assert!(clean.contains(" ab\n"));
    }

    #[test]
    fn a_report_has_every_part_and_no_label_or_note() {
        let alarms = [alarm("Secret errand", "pills in the blue box")];
        let answers = Answers {
            kind: Kind::Problem,
            what: "It did not ring".into(),
            expected: "A ring".into(),
            when: "2026-11-14 07:00".into(),
        };
        let log =
            "2026-11-14T07:00:01.000+01:00 ring started id=aaaaaaaa late=1s\nleak: Secret errand\n";
        let text = build(&answers, "0.5.0", &parts(&alarms, log));
        let app = format!("App: AlarmFree 0.5.0 ({VERSION_CODE})");
        for needle in [
            "Type: Problem",
            app.as_str(),
            "It did not ring",
            "== Device ==",
            "Time zone: Europe/Amsterdam",
            "== Permissions ==",
            "== Alarms (1) ==",
            "== Log ==",
            "ring started",
        ] {
            assert!(text.contains(needle), "missing {needle}");
        }
        assert!(!text.contains("Secret errand"));
        assert!(!text.contains("pills"));
    }

    #[test]
    fn without_the_log_it_says_so_and_has_no_log_lines() {
        let mut p = parts(&[], "T x y\n");
        p.include_log = false;
        let text = build(&Answers::default(), "0.5.0", &p);
        assert!(text.contains("== Log ==\nnot attached"));
        assert!(!text.contains("T x y"));
    }

    #[test]
    fn the_log_in_a_report_is_cut_to_the_last_lines() {
        let log: String = (0..LOG_LINES + 50)
            .map(|n| format!("T x line{n}\n"))
            .collect();
        let text = build(&Answers::default(), "0.5.0", &parts(&[], &log));
        assert!(text.contains(&format!("line{}", LOG_LINES + 49)));
        assert!(!text.contains("line0\n"));
    }

    #[test]
    fn the_last_ring_and_trouble_are_found_in_the_log() {
        let log = "2026-11-14T07:00:01.000+01:00 ring started id=a late=1s\n2026-11-14T09:00:00.000+01:00 start x\n2026-11-14T09:30:00.000+01:00 panic oops\n";
        assert_eq!(last_ring_time(log).as_deref(), Some("2026-11-14 07:00"));
        assert_eq!(last_ring_time("T start x\n"), None);
        assert!(trouble_since(log, ""));
        assert!(trouble_since(log, "2026-11-14T09:00:00.000+01:00"));
        assert!(!trouble_since(log, &newest_stamp(log)));
        assert!(!trouble_since(
            "2026-11-14T07:00:01.000+01:00 ring started x\n",
            ""
        ));
        assert!(trouble_since(
            "2026-11-14T07:00:01.000+01:00 missed id=a\n",
            ""
        ));
    }

    #[test]
    fn the_version_code_is_the_one_in_sentinel_toml() {
        let toml = include_str!("../sentinel.toml");
        assert!(toml.contains(&format!("version_code = {VERSION_CODE}")));
    }
}
