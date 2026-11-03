//! Sound file management for AlarmFree.
//!
//! Manages two categories of alarm sounds:
//!
//! - **Default sounds** — bundled in the APK, extracted to internal storage on first launch
//! - **Custom sounds** — imported by the user via the native file picker
//!
//! Sound references stored in AlarmData:
//!
//! - Default sounds: bare filename stem (e.g. "alarm", "funk", "lofi")
//! - Custom sounds: full filename with extension (e.g. "a1b2c3d4.mp3")

use std::fs;
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// Directory helpers
// ---------------------------------------------------------------------------

/// App's internal files directory (provided by mobile-sentinel).
fn app_files_dir() -> PathBuf {
    mobile_sentinel::app_files_dir()
}

/// Directory where bundled default sounds are stored after extraction.
pub fn default_sounds_dir() -> PathBuf {
    let dir = app_files_dir().join("sounds/default");
    let _ = fs::create_dir_all(&dir);
    dir
}

/// Directory where user-imported custom sounds are stored.
pub fn custom_sounds_dir() -> PathBuf {
    let dir = app_files_dir().join("sounds/custom");
    let _ = fs::create_dir_all(&dir);
    dir
}

// ---------------------------------------------------------------------------
// Asset extraction (first launch)
// ---------------------------------------------------------------------------

/// Extract bundled sounds from APK assets to internal storage.
/// Only copies files that don't already exist (safe to call on every launch).
pub fn extract_bundled_sounds() {
    let count =
        mobile_sentinel::AssetExtractor::new(app_files_dir()).extract("default", "sounds/default");
    if count > 0 {
        log::info!("[AlarmFree] Extracted {count} bundled sounds");
    }
    let removed = prune_superseded_defaults(&default_sounds_dir());
    if removed > 0 {
        log::info!("[AlarmFree] Removed {removed} superseded bundled sound files");
    }
}

// ---------------------------------------------------------------------------
// Sound listing and lookup
// ---------------------------------------------------------------------------

/// Get the list of available default sound names (filename stems, sorted).
pub fn list_default_sounds() -> Vec<String> {
    let dir = default_sounds_dir();
    let mut sounds = Vec::new();
    if let Ok(entries) = fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                    sounds.push(stem.to_string());
                }
            }
        }
    }
    sounds.sort();
    // A stem can briefly exist in two formats (see `prune_superseded_defaults`).
    sounds.dedup();
    sounds
}

/// The bundled sound an alarm falls back to when its own sound and the system
/// default cannot be played.
pub const FALLBACK_SOUND: &str = "happy";

/// Format the bundled default sounds ship in. Older releases shipped `.mp3`.
const BUNDLED_EXTENSION: &str = "ogg";

/// Delete extracted default sounds that a newer format supersedes: when a stem
/// exists as `<stem>.ogg` and also in another format (left over from an older
/// release that bundled `.mp3`), drop the old file. Returns how many files
/// were removed. Alarms reference default sounds by stem, so nothing breaks.
fn prune_superseded_defaults(dir: &std::path::Path) -> usize {
    let Ok(entries) = fs::read_dir(dir) else {
        return 0;
    };
    let files: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_file())
        .collect();
    let current: std::collections::HashSet<String> = files
        .iter()
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some(BUNDLED_EXTENSION))
        .filter_map(|p| p.file_stem()?.to_str().map(str::to_owned))
        .collect();
    files
        .iter()
        .filter(|p| p.extension().and_then(|e| e.to_str()) != Some(BUNDLED_EXTENSION))
        .filter(|p| {
            p.file_stem()
                .and_then(|s| s.to_str())
                .is_some_and(|s| current.contains(s))
        })
        .filter(|p| fs::remove_file(p).is_ok())
        .count()
}

/// Get the full file path for a default sound by its display name (stem).
pub fn get_default_sound_path(name: &str) -> Option<PathBuf> {
    let dir = default_sounds_dir();
    if let Ok(entries) = fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                    if stem == name {
                        return Some(path);
                    }
                }
            }
        }
    }
    None
}

/// Get the playback file path for any sound reference (default or custom).
pub fn get_playback_path(sound: &str) -> Option<PathBuf> {
    if is_custom_sound(sound) {
        let path = custom_sounds_dir().join(sound);
        if path.exists() {
            Some(path)
        } else {
            None
        }
    } else {
        get_default_sound_path(sound)
    }
}

// ---------------------------------------------------------------------------
// Custom sound import and management
// ---------------------------------------------------------------------------

/// A sound the user added: its file, and what it was called where it came from.
pub struct ImportedSound {
    pub file: String,
    pub name: String,
}

/// Launch the native file picker for audio files and import the selected file. `None`
/// when the user changed their mind.
/// MUST be called from a background thread (blocks until user picks or cancels).
pub fn pick_audio_file_native() -> Result<Option<ImportedSound>, String> {
    let picked = mobile_sentinel::media_picker::pick_files(&["audio/*"], false)
        .map_err(|e| format!("File pick failed: {}", e))?;
    let Some(picked) = picked.into_iter().next() else {
        return Ok(None);
    };
    let imported_path = std::path::PathBuf::from(&picked.path);

    // Move from sentinel's generic import directory to our custom sounds directory
    if !imported_path.exists() {
        return Err(format!("Imported file not found at: {:?}", imported_path));
    }
    let raw_filename = imported_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("imported.bin")
        .to_string();
    let filename = crate::sanitize::sanitize_filename(&raw_filename)
        .map_err(|e| format!("Invalid filename: {}", e))?;
    // A sound that cannot play only shows when the alarm rings, which is too late
    // to find out.
    if !looks_like_audio(&imported_path) {
        let _ = fs::remove_file(&imported_path);
        return Err(NOT_A_SOUND.to_owned());
    }
    let dest = custom_sounds_dir().join(&filename);

    // Try rename (fast, same filesystem), fall back to copy+delete
    if fs::rename(&imported_path, &dest).is_err() {
        fs::copy(&imported_path, &dest)
            .map_err(|e| format!("Failed to copy imported file: {}", e))?;
        let _ = fs::remove_file(&imported_path);
    }
    let name = Path::new(&picked.name)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or(&picked.name)
        .to_owned();
    Ok(Some(ImportedSound {
        file: filename,
        name,
    }))
}

/// Why [`pick_audio_file_native`] refused a file that is no sound.
pub const NOT_A_SOUND: &str = "not a sound";

fn looks_like_audio(path: &Path) -> bool {
    let mut head = [0u8; 12];
    let read = fs::File::open(path).and_then(|mut f| std::io::Read::read(&mut f, &mut head));
    read.is_ok_and(|n| is_audio_header(&head[..n]))
}

/// Whether a file starts the way the audio formats Android plays start.
fn is_audio_header(head: &[u8]) -> bool {
    let starts = |magic: &[u8]| head.starts_with(magic);
    starts(b"ID3")
        || starts(b"OggS")
        || starts(b"fLaC")
        || starts(b"#!AMR")
        || starts(&[0x1A, 0x45, 0xDF, 0xA3]) // Matroska / WebM
        || (starts(b"RIFF") && head.get(8..12) == Some(b"WAVE".as_slice()))
        || head.get(4..8) == Some(b"ftyp".as_slice()) // MP4 / M4A
        || matches!(head, [0xFF, second, ..] if second & 0xE0 == 0xE0) // MP3 / AAC frame
}

/// The user's own sounds, by filename.
pub fn list_custom_sounds() -> Vec<String> {
    let mut sounds: Vec<String> = fs::read_dir(custom_sounds_dir())
        .map(|entries| {
            entries
                .flatten()
                .filter(|entry| entry.path().is_file())
                .filter_map(|entry| entry.file_name().into_string().ok())
                .collect()
        })
        .unwrap_or_default();
    sounds.sort();
    sounds
}

/// Whether this phone has the sound an alarm refers to.
pub fn sound_exists(sound: &str) -> bool {
    sound.is_empty() || get_playback_path(sound).is_some()
}

/// Delete a custom sound file by its unique filename.
pub fn delete_sound(filename: &str) {
    if filename.is_empty() {
        return;
    }
    let _ = fs::remove_file(custom_sounds_dir().join(filename));
}

// ---------------------------------------------------------------------------
// Sound reference helpers
// ---------------------------------------------------------------------------

/// Check if a sound reference is a custom (user-imported) sound.
/// Default sounds are bare stems without extensions ("alarm", "funk").
/// Custom sounds have file extensions ("a1b2c3d4.mp3").
pub fn is_custom_sound(sound: &str) -> bool {
    if sound.is_empty() {
        return false;
    }
    sound.contains('.')
}

/// The name to show for a sound: a bundled one is capitalised ("New world"), the user's
/// own has the name they gave it, or else its file name.
pub fn display_name(sound: &str, names: &std::collections::BTreeMap<String, String>) -> String {
    if is_custom_sound(sound) {
        if let Some(name) = names.get(sound) {
            return name.clone();
        }
        return Path::new(sound)
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or(sound)
            .to_owned();
    }
    let spaced = sound.replace(['_', '-'], " ");
    let mut chars = spaced.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn a_file_is_only_taken_for_a_sound_if_it_starts_like_one() {
        for good in [
            b"ID3\x03\x00".as_slice(),
            b"OggS\x00",
            b"fLaC",
            b"RIFF\x10\x00\x00\x00WAVE",
            b"\x00\x00\x00\x18ftypM4A ",
            &[0xFF, 0xFB, 0x90],
        ] {
            assert!(is_audio_header(good), "{good:?}");
        }
        for bad in [
            b"".as_slice(),
            b"not-audio",
            b"%PDF-1.7",
            b"RIFF\x10\x00\x00\x00AVI ",
            b"\x89PNG\r\n",
        ] {
            assert!(!is_audio_header(bad), "{bad:?}");
        }
    }

    #[test]
    fn prune_removes_old_format_only_when_new_format_exists() {
        let dir = TempDir::new().unwrap();
        for f in ["power.mp3", "power.ogg", "funk.mp3", "lofi.ogg"] {
            fs::write(dir.path().join(f), b"x").unwrap();
        }
        assert_eq!(prune_superseded_defaults(dir.path()), 1);
        let mut left: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        left.sort();
        // `funk.mp3` has no replacement, so it stays playable.
        assert_eq!(left, ["funk.mp3", "lofi.ogg", "power.ogg"]);
    }

    #[test]
    fn a_sound_shows_the_name_it_was_given_or_else_its_file_name() {
        let mut names = std::collections::BTreeMap::new();
        assert_eq!(display_name("new world", &names), "New world");
        assert_eq!(display_name("my tune.mp3", &names), "my tune");
        names.insert("my tune.mp3".to_owned(), "Morning".to_owned());
        assert_eq!(display_name("my tune.mp3", &names), "Morning");
    }

    #[test]
    fn prune_handles_missing_dir() {
        assert_eq!(
            prune_superseded_defaults(std::path::Path::new("/no/such/dir")),
            0
        );
    }
}
