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
use std::path::PathBuf;

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
fn custom_sounds_dir() -> PathBuf {
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
    let extractor = mobile_sentinel::AssetExtractor::new(app_files_dir());

    #[cfg(target_os = "android")]
    {
        let count = extractor.extract("default", "sounds/default");
        if count > 0 {
            log::info!("[AlarmFree] Extracted {} bundled sounds", count);
        }
        let removed = prune_superseded_defaults(&default_sounds_dir());
        if removed > 0 {
            log::info!(
                "[AlarmFree] Removed {} superseded bundled sound files",
                removed
            );
        }
    }

    #[cfg(not(target_os = "android"))]
    {
        let src = PathBuf::from("apps/alarmfree/assets/sounds/default");
        if src.exists() {
            let count = extractor.extract_from_dir(&src, "sounds/default");
            if count > 0 {
                log::info!("[AlarmFree] Copied {} bundled sounds from source", count);
            }
        }
        prune_superseded_defaults(&default_sounds_dir());
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

/// Launch the native file picker for audio files and import the selected file.
/// Returns the unique filename on success.
/// MUST be called from a background thread (blocks until user picks or cancels).
pub fn pick_audio_file_native() -> Result<String, String> {
    // Launch the native file picker via the gated media_picker capability
    // (blocks until user selects or cancels).
    let imported = mobile_sentinel::media_picker::pick_file(&["audio/*"])
        .map_err(|e| format!("File pick failed: {}", e))?;
    let imported_path = std::path::PathBuf::from(imported);

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
    let dest = custom_sounds_dir().join(&filename);

    // Try rename (fast, same filesystem), fall back to copy+delete
    if fs::rename(&imported_path, &dest).is_err() {
        fs::copy(&imported_path, &dest)
            .map_err(|e| format!("Failed to copy imported file: {}", e))?;
        let _ = fs::remove_file(&imported_path);
    }
    Ok(filename)
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

/// Translation-aware display name for any sound reference. Custom sounds are
/// wrapped in the localized "Custom (...)" label (truncated past 12 chars);
/// bundled/system references return their raw string unchanged.
pub fn display_name_translated(sound: &str, translations: &crate::app::Translations) -> String {
    if is_custom_sound(sound) {
        let truncated: String = sound.chars().take(12).collect();
        let key = if sound.len() > 12 {
            "sound.custom_name_truncated"
        } else {
            "sound.custom_name"
        };
        crate::app::t(translations, key).replace("{name}", &truncated)
    } else {
        sound.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

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
    fn prune_handles_missing_dir() {
        assert_eq!(
            prune_superseded_defaults(std::path::Path::new("/no/such/dir")),
            0
        );
    }
}
