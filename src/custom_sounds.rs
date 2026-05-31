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
            eprintln!("[AlarmFree] Extracted {} bundled sounds", count);
        }
    }

    #[cfg(not(target_os = "android"))]
    {
        let src = PathBuf::from("apps/alarmfree/assets/sounds/default");
        if src.exists() {
            let count = extractor.extract_from_dir(&src, "sounds/default");
            if count > 0 {
                eprintln!("[AlarmFree] Copied {} bundled sounds from source", count);
            }
        }
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
    sounds
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
