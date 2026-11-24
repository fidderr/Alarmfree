//! The user's own photos for the ring screen. Shrunk when they come in, so that at seven in the
//! morning the screen has nothing heavy to load. A photo that is gone is the plain screen.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::models::AlarmData;

/// Plenty for a phone's screen, and a few hundred kilobytes.
const LONGEST_SIDE: u32 = 1600;
/// Nothing this app writes comes near it: not something to put on the page.
const MAX_BYTES: u64 = 4 * 1024 * 1024;

fn dir() -> PathBuf {
    mobile_sentinel::app_files_dir().join("ring_photos")
}

/// Only a bare file name: a backup file comes from outside.
fn is_bare_name(name: &str) -> bool {
    !name.contains('\\') && Path::new(name).file_name().is_some_and(|file| file == name)
}

/// Let the user pick a photo, and keep a small copy. Returns its name, `None` if they changed
/// their mind. Blocks until they are done, so not from the UI thread.
pub fn import() -> Result<Option<String>, String> {
    let picked = mobile_sentinel::media_picker::pick_files(&["image/*"], false)
        .map_err(|e| e.to_string())?;
    let Some(picked) = picked.into_iter().next() else {
        return Ok(None);
    };
    std::fs::create_dir_all(dir()).map_err(|e| e.to_string())?;
    let name = format!("{}.jpg", uuid::Uuid::new_v4());
    let target = dir().join(&name);
    let shrunk = mobile_sentinel::media_picker::shrink_image(
        &picked.path,
        &target.to_string_lossy(),
        LONGEST_SIDE,
    );
    let _ = std::fs::remove_file(&picked.path);
    shrunk.map_err(|e| e.to_string())?;
    Ok(Some(name))
}

/// The last one read: this is asked at every render, and a file only once.
static SHOWN: Mutex<Option<(String, String)>> = Mutex::new(None);

/// A photo the way the page takes it, if it can be read.
pub fn data_url(name: &str) -> Option<String> {
    let mut shown = SHOWN.lock().ok()?;
    if let Some((shown_name, url)) = shown.as_ref() {
        if shown_name == name {
            return Some(url.clone());
        }
    }
    if !is_bare_name(name) {
        return None;
    }
    let path = dir().join(name);
    if std::fs::metadata(&path).ok()?.len() > MAX_BYTES {
        return None;
    }
    let url = format!(
        "data:image/jpeg;base64,{}",
        crate::base64::encode(&std::fs::read(path).ok()?)
    );
    *shown = Some((name.to_owned(), url.clone()));
    Some(url)
}

/// The file of a photo, if it can be read: for a view that loads from a path.
pub fn path(name: &str) -> Option<PathBuf> {
    if !is_bare_name(name) {
        return None;
    }
    let path = dir().join(name);
    (std::fs::metadata(&path).ok()?.len() <= MAX_BYTES).then_some(path)
}

/// Throw away the photos nothing uses any more. An alarm deleted a moment ago can still be brought
/// back, so this is for when the app starts. With no alarms the database may just not be readable
/// yet: then it does nothing.
pub fn prune(alarms: &[AlarmData], defaults: Option<&AlarmData>) {
    if alarms.is_empty() {
        return;
    }
    let in_use: Vec<&str> = alarms
        .iter()
        .chain(defaults)
        .filter_map(|alarm| alarm.ring_look.photo.as_deref())
        .collect();
    let Ok(files) = std::fs::read_dir(dir()) else {
        return;
    };
    for file in files.flatten() {
        let name = file.file_name();
        if !in_use.iter().any(|used| name == *used) {
            let _ = std::fs::remove_file(file.path());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_from_a_backup_cannot_point_anywhere_else() {
        assert!(is_bare_name("a.jpg"));
        assert!(!is_bare_name("../alarmfree.db"));
        assert!(!is_bare_name("sub/a.jpg"));
        assert!(!is_bare_name("..\\a.jpg"));
        assert!(!is_bare_name(".."));
        assert!(!is_bare_name(""));
    }

    #[test]
    fn a_photo_that_is_gone_is_nothing_to_show() {
        assert_eq!(data_url("no-such-photo.jpg"), None);
    }

    #[test]
    fn no_alarms_is_no_reason_to_throw_anything_away() {
        // Would be a disaster if it did: nothing to assert but that it stays away from the disk.
        prune(&[], None);
    }
}
