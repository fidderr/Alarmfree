//! The backup file. Without own sounds in use it is a list of the alarms. With them it is an
//! object `{"alarms": [...], "sounds": [{"file", "name", "data"}]}`: the names of the sounds
//! the alarms use, so a restore can say which are missing, and, when the user asked for it,
//! each sound's file as base64 in `data`. A file of either kind is read back; AlarmFree 0.7.1
//! reads only the list and says "not a backup" to the object.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};

use crate::alarm_ops;
use crate::custom_sounds;
use crate::i18n::{t, Translations};
use crate::models::AlarmData;

#[derive(Serialize, Deserialize)]
struct SoundEntry {
    file: String,
    name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    data: Option<String>,
}

#[derive(Serialize)]
struct Written<'a> {
    alarms: Vec<&'a AlarmData>,
    sounds: Vec<SoundEntry>,
}

#[derive(Deserialize)]
struct Read {
    alarms: Vec<AlarmData>,
    #[serde(default)]
    sounds: Vec<SoundEntry>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum Content {
    List(Vec<AlarmData>),
    WithSounds(Read),
}

/// The user's own sounds that the alarms of a backup use, once each.
pub fn sounds_used(alarms: &[AlarmData]) -> Vec<String> {
    let used: BTreeSet<&str> = alarm_ops::backup_alarms(alarms)
        .into_iter()
        .map(|a| a.sound.as_str())
        .filter(|sound| custom_sounds::is_custom_sound(sound))
        .collect();
    used.into_iter().map(str::to_owned).collect()
}

/// The text of the backup file. `with_sounds` puts the sound files in it (those that are on this
/// phone).
pub fn export(
    alarms: &[AlarmData],
    names: &BTreeMap<String, String>,
    sounds_dir: &Path,
    with_sounds: bool,
) -> String {
    let used = sounds_used(alarms);
    if used.is_empty() {
        return alarm_ops::export_json(alarms);
    }
    let sounds = used
        .into_iter()
        .map(|file| SoundEntry {
            name: custom_sounds::display_name(&file, names),
            data: with_sounds
                .then(|| std::fs::read(sounds_dir.join(&file)).ok())
                .flatten()
                .map(|bytes| crate::base64::encode(&bytes)),
            file,
        })
        .collect();
    let written = Written {
        alarms: alarm_ops::backup_alarms(alarms),
        sounds,
    };
    serde_json::to_string_pretty(&written).unwrap_or_else(|_| "[]".to_owned())
}

/// About how many bytes the file is with the sounds in it, for telling the user before it is saved.
pub fn size_with_sounds(
    alarms: &[AlarmData],
    names: &BTreeMap<String, String>,
    sounds_dir: &Path,
) -> u64 {
    let without = export(alarms, names, sounds_dir, false).len() as u64;
    let data: u64 = sounds_used(alarms)
        .iter()
        .filter_map(|file| std::fs::metadata(sounds_dir.join(file)).ok())
        .map(|m| m.len().div_ceil(3) * 4 + 12)
        .sum();
    without + data
}

/// What a backup file gave.
pub struct Restored {
    /// The alarms, ready to be stored.
    pub alarms: Vec<AlarmData>,
    /// Sounds that were put on this phone: the file, and the name to show for it.
    pub names: Vec<(String, String)>,
    /// The names of sounds the alarms use that are not on this phone.
    pub missing: Vec<String>,
    /// How many such sounds the file has no name for (a file of an older version).
    pub missing_unnamed: usize,
}

/// Read a backup file. The sounds in it are written to `sounds_dir` first; `known_sound` is then
/// asked about what the alarms refer to, and what is still not there falls back to the first bundled
/// sound and is reported in `missing`. `None` when the file is no backup.
pub fn import(
    json: &str,
    sounds_dir: &Path,
    known_sound: impl Fn(&str) -> bool,
    now: NaiveDateTime,
) -> Option<Restored> {
    let (mut alarms, sounds) = match serde_json::from_str(json).ok()? {
        Content::List(alarms) => (alarms, Vec::new()),
        Content::WithSounds(file) => (file.alarms, file.sounds),
    };
    let mut placed = BTreeMap::new();
    let mut names = Vec::new();
    let mut shown = BTreeMap::new();
    for entry in &sounds {
        shown.insert(entry.file.as_str(), entry.name.as_str());
        let bytes = entry.data.as_deref().and_then(crate::base64::decode);
        let stored =
            bytes.and_then(|bytes| custom_sounds::place_sound(sounds_dir, &entry.file, &bytes));
        if let Some((stored, written)) = stored {
            if written && !entry.name.is_empty() {
                names.push((stored.clone(), entry.name.clone()));
            }
            placed.insert(entry.file.as_str(), stored);
        }
    }
    for alarm in &mut alarms {
        if let Some(stored) = placed.get(alarm.sound.as_str()) {
            alarm.sound.clone_from(stored);
        }
    }
    let mut missing = BTreeSet::new();
    let mut unnamed = BTreeSet::new();
    for alarm in alarms.iter().filter(|a| !a.delete_after_ring) {
        let sound = alarm.sound.as_str();
        if custom_sounds::is_custom_sound(sound) && !known_sound(sound) {
            match shown.get(sound).filter(|name| !name.is_empty()) {
                Some(name) => missing.insert((*name).to_owned()),
                None => unnamed.insert(sound.to_owned()),
            };
        }
    }
    Some(Restored {
        alarms: alarm_ops::restore_alarms(alarms, known_sound, now),
        names,
        missing: missing.into_iter().collect(),
        missing_unnamed: unnamed.len(),
    })
}

/// The sentence that names the sounds a restore could not bring, if there are any.
pub fn missing_sounds_text(
    tr: &Translations,
    missing: &[String],
    missing_unnamed: usize,
) -> Option<String> {
    const SHOWN: usize = 3;
    if !missing.is_empty() {
        let mut names = missing
            .iter()
            .take(SHOWN)
            .cloned()
            .collect::<Vec<_>>()
            .join(", ");
        if missing.len() > SHOWN {
            names.push_str(", …");
        }
        return Some(t(tr, "settings.backup_sounds_missing").replace("{names}", &names));
    }
    (missing_unnamed > 0).then(|| t(tr, "settings.backup_sounds_missing_unnamed"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;
    use tempfile::TempDir;

    fn now() -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 11, 3)
            .unwrap()
            .and_hms_opt(7, 0, 0)
            .unwrap()
    }

    fn alarm_with(sound: &str) -> AlarmData {
        let mut alarm = alarm_ops::factory_alarm();
        alarm.sound = sound.to_owned();
        alarm
    }

    fn names(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(file, name)| ((*file).to_owned(), (*name).to_owned()))
            .collect()
    }

    fn there(dir: &Path) -> impl Fn(&str) -> bool + '_ {
        |sound| !custom_sounds::is_custom_sound(sound) || dir.join(sound).exists()
    }

    /// Two phones: what the first one saves, the second one reads.
    fn two_phones() -> (TempDir, TempDir) {
        (TempDir::new().unwrap(), TempDir::new().unwrap())
    }

    const TUNE: &[u8] = b"OggS the sound of a morning";

    #[test]
    fn without_own_sounds_the_file_is_the_plain_list_as_before() {
        let dir = TempDir::new().unwrap();
        let alarms = [alarm_with("happy")];
        for with in [false, true] {
            let json = export(&alarms, &BTreeMap::new(), dir.path(), with);
            assert_eq!(json, alarm_ops::export_json(&alarms));
            assert!(json.trim_start().starts_with('['));
        }
    }

    #[test]
    fn alarms_only_names_the_sounds_but_holds_no_files() {
        let (first, second) = two_phones();
        std::fs::write(first.path().join("a-b.ogg"), TUNE).unwrap();
        let alarms = [alarm_with("a-b.ogg"), alarm_with("a-b.ogg")];
        let json = export(
            &alarms,
            &names(&[("a-b.ogg", "Morning")]),
            first.path(),
            false,
        );
        assert!(!json.contains("\"data\""), "{json}");
        let back = import(&json, second.path(), there(second.path()), now()).unwrap();
        assert_eq!(back.alarms.len(), 2);
        assert_eq!(back.missing, ["Morning"]);
        assert_eq!(back.missing_unnamed, 0);
        assert_eq!(back.alarms[0].sound, alarm_ops::factory_alarm().sound);
        assert!(back.names.is_empty());
        assert!(std::fs::read_dir(second.path()).unwrap().next().is_none());
    }

    #[test]
    fn with_sounds_they_arrive_on_the_other_phone_under_their_names() {
        let (first, second) = two_phones();
        std::fs::write(first.path().join("a-b.ogg"), TUNE).unwrap();
        let alarms = [alarm_with("a-b.ogg"), alarm_with("happy")];
        let json = export(
            &alarms,
            &names(&[("a-b.ogg", "Morning")]),
            first.path(),
            true,
        );
        let back = import(&json, second.path(), there(second.path()), now()).unwrap();
        assert!(back.missing.is_empty());
        assert_eq!(back.alarms[0].sound, "a-b.ogg");
        assert_eq!(back.alarms[1].sound, "happy");
        assert_eq!(std::fs::read(second.path().join("a-b.ogg")).unwrap(), TUNE);
        assert_eq!(back.names, [("a-b.ogg".to_owned(), "Morning".to_owned())]);
    }

    #[test]
    fn a_sound_that_is_already_there_is_not_added_a_second_time() {
        let (first, second) = two_phones();
        std::fs::write(first.path().join("a-b.ogg"), TUNE).unwrap();
        // The same bytes under another name on the second phone, and the same name too.
        std::fs::write(second.path().join("mine.ogg"), TUNE).unwrap();
        let json = export(
            &[alarm_with("a-b.ogg")],
            &names(&[("a-b.ogg", "Morning")]),
            first.path(),
            true,
        );
        let back = import(&json, second.path(), there(second.path()), now()).unwrap();
        assert_eq!(back.alarms[0].sound, "mine.ogg");
        assert_eq!(std::fs::read_dir(second.path()).unwrap().count(), 1);
        assert!(back.names.is_empty());
        let again = import(&json, second.path(), there(second.path()), now()).unwrap();
        assert_eq!(again.alarms[0].sound, "mine.ogg");
    }

    #[test]
    fn another_sound_with_the_same_file_name_is_kept_and_the_alarm_points_to_the_new_one() {
        let (first, second) = two_phones();
        std::fs::write(first.path().join("a-b.ogg"), TUNE).unwrap();
        std::fs::write(second.path().join("a-b.ogg"), b"OggS something else").unwrap();
        let json = export(
            &[alarm_with("a-b.ogg")],
            &names(&[("a-b.ogg", "Morning")]),
            first.path(),
            true,
        );
        let back = import(&json, second.path(), there(second.path()), now()).unwrap();
        assert_eq!(back.alarms[0].sound, "a-b-2.ogg");
        assert_eq!(
            std::fs::read(second.path().join("a-b.ogg")).unwrap(),
            b"OggS something else"
        );
        assert_eq!(
            std::fs::read(second.path().join("a-b-2.ogg")).unwrap(),
            TUNE
        );
        assert_eq!(back.names, [("a-b-2.ogg".to_owned(), "Morning".to_owned())]);
    }

    #[test]
    fn a_sound_that_is_on_the_phone_is_not_missing_in_a_file_without_files() {
        let (first, second) = two_phones();
        std::fs::write(first.path().join("a-b.ogg"), TUNE).unwrap();
        std::fs::write(second.path().join("a-b.ogg"), TUNE).unwrap();
        let json = export(
            &[alarm_with("a-b.ogg")],
            &BTreeMap::new(),
            first.path(),
            false,
        );
        let back = import(&json, second.path(), there(second.path()), now()).unwrap();
        assert!(back.missing.is_empty());
        assert_eq!(back.alarms[0].sound, "a-b.ogg");
    }

    #[test]
    fn a_file_of_an_older_version_is_read_and_its_missing_sounds_are_counted() {
        let dir = TempDir::new().unwrap();
        let json = alarm_ops::export_json(&[
            alarm_with("1234.ogg"),
            alarm_with("5678.ogg"),
            alarm_with("happy"),
        ]);
        let back = import(&json, dir.path(), there(dir.path()), now()).unwrap();
        assert_eq!(back.alarms.len(), 3);
        assert!(back.missing.is_empty());
        assert_eq!(back.missing_unnamed, 2);
        let factory = alarm_ops::factory_alarm().sound;
        assert!(back.alarms[..2].iter().all(|a| a.sound == factory));
    }

    #[test]
    fn what_comes_in_a_file_cannot_write_outside_the_sounds_folder_or_be_no_sound() {
        let (first, second) = two_phones();
        let mut entries = Vec::new();
        for (file, bytes) in [
            ("../escape.ogg", TUNE),
            ("sub/escape.ogg", TUNE),
            ("noextension", TUNE),
            ("fake.ogg", b"%PDF-1.7".as_slice()),
        ] {
            entries.push(serde_json::json!({
                "file": file,
                "name": "x",
                "data": crate::base64::encode(bytes),
            }));
        }
        let json = serde_json::json!({
            "alarms": [alarm_with("fake.ogg")],
            "sounds": entries,
        })
        .to_string();
        let back = import(&json, second.path(), there(second.path()), now()).unwrap();
        assert!(std::fs::read_dir(second.path()).unwrap().next().is_none());
        assert!(!first.path().join("escape.ogg").exists());
        assert_eq!(back.missing, ["x"]);
    }

    #[test]
    fn the_size_told_before_saving_is_about_the_size_of_the_file() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("a-b.ogg"), vec![b'O'; 30_000]).unwrap();
        let alarms = [alarm_with("a-b.ogg")];
        let told = size_with_sounds(&alarms, &BTreeMap::new(), dir.path());
        let written = export(&alarms, &BTreeMap::new(), dir.path(), true).len() as u64;
        assert!(told.abs_diff(written) < 100, "{told} against {written}");
        assert!(told > 40_000);
    }

    #[test]
    fn a_wake_up_check_and_a_test_alarm_bring_no_sound() {
        let mut hidden = alarm_with("a-b.ogg");
        hidden.hidden = true;
        assert!(sounds_used(&[hidden, alarm_with("happy")]).is_empty());
    }

    #[test]
    fn the_missing_sounds_are_named_a_few_at_a_time() {
        let en = crate::i18n::load_translations(crate::models::Locale::English);
        assert_eq!(missing_sounds_text(&en, &[], 0), None);
        let one = missing_sounds_text(&en, &["Morning".to_owned()], 0).unwrap();
        assert!(one.contains("Morning") && !one.contains('{'), "{one}");
        let many: Vec<String> = ["A", "B", "C", "D"].map(str::to_owned).to_vec();
        assert!(missing_sounds_text(&en, &many, 0)
            .unwrap()
            .contains("A, B, C, …"));
        let unnamed = missing_sounds_text(&en, &[], 2).unwrap();
        assert!(!unnamed.contains('{'));
    }
}
