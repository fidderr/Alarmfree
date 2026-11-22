//! What the test scenarios (`devtest/`) drive the app with. Only in a debug build (this module is
//! not compiled in a release), never in one for users.
//!
//! One request at a time. The harness writes `<files>/debug_request.txt` (first to a `.tmp` file,
//! then a rename): one line, `<id> <command>`. The id is made up by the harness (digits and
//! dashes). The app takes the file by renaming it to `debug_request.taking` and answers in
//! `<files>/debug_answer_<id>.txt`; the harness reads exactly that file and removes it. An
//! answer nobody waits for, and the mark `debug_dead` (the harness writes it when a request it
//! saw taken got no answer, and then sends nothing more), are removed when the app starts; the
//! app also writes `debug_proc`, a name for this process, before it removes anything. A
//! request that is waiting then is taken by the new process: the harness takes it back itself
//! when it waited too long. Every command is answered: the commands that act answer `OK` (or
//! `ERR ...`) when taken.
//!
//!   `snooze:<id>`, `dismiss:<id>`: the same calls as the screens make.
//!   `hang_ui`: freeze the UI thread, to exercise the guardian.
//!   `squat_record:start`, `squat_record:stop`: record the raw accelerometer to
//!     `<files>/squat_record.csv` while the squat counter runs (for tuning it with real squats,
//!     `devtest/squat_record.sh`); stop leaves `<files>/squat_record.txt` with the sample and squat
//!     counts.
//!   `ui:<command>`: ask or press something in the Slint window, see `ui/debug.rs`,
//!     `ui/debug_elements.rs` and `ui/ready.rs` for the commands (`screen`, `snapshot`, `ready`,
//!     `await`, `press:<callback>:<arg>`, `texts`, `find`, `tap`, ...). The answer is one line, or JSON.
//!
//! `<files>/debug_<name>.txt` holds a number to use instead of a wait that takes too long
//! for a test, see [`crate::platform::shortened`].

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use mobile_sentinel::InstanceId;

use crate::platform::alarm_kit_optional;

static HANG_UI: AtomicBool = AtomicBool::new(false);

/// Where an answer goes once it is known; called from any thread.
type Reply = Box<dyn FnOnce(String) + Send>;

/// Watch for what the harness writes.
pub fn start() {
    let dir = mobile_sentinel::app_files_dir();
    // First, so a harness that sees the mark gone sees this process too.
    write_process_mark(&dir);
    remove_leftovers(&dir);
    std::thread::spawn(move || loop {
        std::thread::sleep(std::time::Duration::from_millis(30));
        if let Some(text) = take_request(&dir) {
            dispatch(&dir, &text, run_ui);
        }
    });
}

/// `debug_proc` names this process (digits and a dash). The harness reads it when it sends a request
/// and again when it gave up: only a request that the same process may still run closes its channel.
fn write_process_mark(dir: &Path) {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let tmp = dir.join("debug_proc.tmp");
    if std::fs::write(&tmp, format!("{}-{nanos}\n", std::process::id())).is_ok() {
        let _ = std::fs::rename(&tmp, dir.join("debug_proc"));
    }
}

/// What an earlier process left: a request it had taken, a mark that it never answered, answers
/// nobody read. A request that waits for this process stays.
fn remove_leftovers(dir: &Path) {
    let _ = std::fs::remove_file(dir.join("debug_request.taking"));
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with("debug_answer_")
            || name == "debug_request.withdrawn"
            || name == "debug_dead"
        {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

/// The request the harness left, if any. It is claimed by a rename, so the harness can tell with
/// one rename of its own whether the app took it.
fn take_request(dir: &Path) -> Option<String> {
    let path = dir.join("debug_request.txt");
    let taking = dir.join("debug_request.taking");
    std::fs::rename(&path, &taking).ok()?;
    let text = std::fs::read_to_string(&taking).ok();
    let _ = std::fs::remove_file(&taking);
    text
}

/// `<id> <command>`: the id is digits and dashes, so it is safe in a file name.
fn parse_request(text: &str) -> Option<(&str, &str)> {
    let (id, command) = text.trim().split_once(' ')?;
    let valid = !id.is_empty() && id.bytes().all(|b| b.is_ascii_digit() || b == b'-');
    valid.then_some((id, command.trim()))
}

fn write_answer(dir: &Path, id: &str, answer: &str) {
    let tmp = dir.join(format!("debug_answer_{id}.tmp"));
    if std::fs::write(&tmp, answer).is_ok() {
        let _ = std::fs::rename(&tmp, dir.join(format!("debug_answer_{id}.txt")));
    }
}

/// Run a `ui:` command on the UI thread; `reply` gets the answer.
fn run_ui(command: String, reply: Reply) {
    crate::ui::debug_command(command, reply);
}

/// Answer one request: `ui:` commands through `run_ui` (the answer comes when the UI thread has
/// run it), the others at once.
fn dispatch(dir: &Path, text: &str, run_ui: fn(String, Reply)) {
    let Some((id, command)) = parse_request(text) else {
        log::warn!("[AlarmFree] DEBUG request without an id: {text}");
        return;
    };
    let dir = dir.to_path_buf();
    let id = id.to_owned();
    match command.strip_prefix("ui:") {
        Some(ui) => {
            let pending = Pending {
                dir,
                id,
                done: false,
            };
            run_ui(
                ui.to_owned(),
                Box::new(move |answer| pending.answer(&answer)),
            );
        }
        None => write_answer(&dir, &id, &run_action(command)),
    }
}

/// A request that was taken and is not answered yet. When it is dropped unanswered (the event loop
/// is gone or never ran the command) it answers with an error: a request that was taken and gets
/// no answer closes the harness's channel for 30 s and the rest of the scenario.
struct Pending {
    dir: PathBuf,
    id: String,
    done: bool,
}

impl Pending {
    fn answer(mut self, answer: &str) {
        write_answer(&self.dir, &self.id, answer);
        self.done = true;
    }
}

impl Drop for Pending {
    fn drop(&mut self) {
        if !self.done {
            write_answer(&self.dir, &self.id, "ERR the UI did not run the command");
        }
    }
}

/// The commands that act on the app. The answer is `OK` once the command was taken (the work of
/// `hang_ui` happens at the next [`block_if_requested`]).
fn run_action(command: &str) -> String {
    if let Some(what) = command.strip_prefix("squat_record:") {
        return squat_record(what);
    }
    if command == "hang_ui" {
        HANG_UI.store(true, Ordering::SeqCst);
        return "OK".to_owned();
    }
    let (kind, id) = command.split_once(':').unwrap_or((command, ""));
    if !matches!(kind, "snooze" | "dismiss") {
        return format!("ERR unknown command {command}");
    }
    let Some(kit) = alarm_kit_optional() else {
        return "ERR the alarm kit is not up".to_owned();
    };
    let id = InstanceId::new(id.to_owned());
    let done = if kind == "snooze" {
        kit.snooze(&id)
    } else {
        kit.dismiss(&id)
    };
    match done {
        Ok(_) => "OK".to_owned(),
        Err(e) => {
            log::warn!("[AlarmFree] DEBUG {kind}({}) failed: {e:?}", id.as_str());
            format!("ERR {kind} failed: {e:?}")
        }
    }
}

/// Start or stop recording the accelerometer next to the squat counter.
fn squat_record(what: &str) -> String {
    let dir = mobile_sentinel::app_files_dir();
    match what {
        "start" => {
            mobile_sentinel::sensors::start_squat_counter();
            let path = dir.join("squat_record.csv");
            let started = mobile_sentinel::sensors::start_recording(&path.to_string_lossy());
            log::info!("[AlarmFree] DEBUG squat_record started: {started}");
        }
        "stop" => {
            let squats = mobile_sentinel::sensors::squat_count();
            let samples = mobile_sentinel::sensors::stop_recording();
            mobile_sentinel::sensors::stop_squat_counter();
            let _ = std::fs::write(
                dir.join("squat_record.txt"),
                format!(
                    "samples={samples} squats={squats}
"
                ),
            );
            log::info!(
                "[AlarmFree] DEBUG squat_record stopped: {samples} samples, {squats} squats"
            );
        }
        other => {
            log::warn!("[AlarmFree] DEBUG squat_record: unknown {other}");
            return format!("ERR squat_record: unknown {other}");
        }
    }
    "OK".to_owned()
}

/// The number the harness left for `name`, if any.
pub fn test_value(name: &str) -> Option<u32> {
    let path = mobile_sentinel::app_files_dir().join(format!("debug_{name}.txt"));
    std::fs::read_to_string(path).ok()?.trim().parse().ok()
}

/// Freeze the calling thread, the UI's, if `hang_ui` asked for it.
pub fn block_if_requested() {
    if HANG_UI.swap(false, Ordering::SeqCst) {
        log::warn!("[AlarmFree] DEBUG blocking the UI thread");
        std::thread::sleep(std::time::Duration::from_secs(3600));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn answer(dir: &Path, id: &str) -> Option<String> {
        std::fs::read_to_string(dir.join(format!("debug_answer_{id}.txt"))).ok()
    }

    /// A stand-in for the UI thread: answers with what it was asked.
    fn echo_ui(command: String, reply: Reply) {
        reply(format!("ui says {command}"));
    }

    #[test]
    fn a_request_is_answered_in_the_file_with_its_id() {
        let dir = tempfile::tempdir().unwrap();
        dispatch(dir.path(), "17-3 ui:screen", echo_ui);
        dispatch(dir.path(), "18-1 ui:ready", echo_ui);
        assert_eq!(
            answer(dir.path(), "17-3").as_deref(),
            Some("ui says screen")
        );
        assert_eq!(answer(dir.path(), "18-1").as_deref(), Some("ui says ready"));
        // Nothing else was written, and no answer without the id's own file.
        assert_eq!(answer(dir.path(), "17-4"), None);
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 2);
    }

    #[test]
    fn a_late_answer_stays_in_its_own_file() {
        let dir = tempfile::tempdir().unwrap();
        // Request 1 is answered after request 2.
        static LATE: std::sync::Mutex<Option<Reply>> = std::sync::Mutex::new(None);
        fn hold(_: String, reply: Reply) {
            *LATE.lock().unwrap() = Some(reply);
        }
        dispatch(dir.path(), "1 ui:snapshot", hold);
        let first = LATE.lock().unwrap().take().unwrap();
        dispatch(dir.path(), "2 ui:screen", echo_ui);
        first("the snapshot".to_owned());
        assert_eq!(answer(dir.path(), "2").as_deref(), Some("ui says screen"));
        assert_eq!(answer(dir.path(), "1").as_deref(), Some("the snapshot"));
    }

    #[test]
    fn a_ui_command_that_never_runs_is_still_answered() {
        let dir = tempfile::tempdir().unwrap();
        // The event loop is gone: the command is dropped with its reply.
        fn drop_it(_: String, reply: Reply) {
            drop(reply);
        }
        dispatch(dir.path(), "7-1 ui:screen", drop_it);
        assert_eq!(
            answer(dir.path(), "7-1").as_deref(),
            Some("ERR the UI did not run the command")
        );
        // An answered request is not answered twice.
        dispatch(dir.path(), "7-2 ui:screen", echo_ui);
        assert_eq!(answer(dir.path(), "7-2").as_deref(), Some("ui says screen"));
    }

    #[test]
    fn action_commands_are_acknowledged() {
        let dir = tempfile::tempdir().unwrap();
        dispatch(dir.path(), "5-1 hang_ui", echo_ui);
        assert_eq!(answer(dir.path(), "5-1").as_deref(), Some("OK"));
        // Do not leave the UI thread of a later test frozen.
        HANG_UI.store(false, Ordering::SeqCst);
        dispatch(dir.path(), "5-2 nonsense", echo_ui);
        assert_eq!(
            answer(dir.path(), "5-2").as_deref(),
            Some("ERR unknown command nonsense")
        );
        // Without the alarm kit (a test has none) a snooze says so instead of staying silent.
        dispatch(dir.path(), "5-3 snooze:a", echo_ui);
        assert!(answer(dir.path(), "5-3").unwrap().starts_with("ERR"));
    }

    #[test]
    fn a_request_needs_an_id() {
        assert_eq!(
            parse_request("12-5 ui:screen\n"),
            Some(("12-5", "ui:screen"))
        );
        assert_eq!(parse_request("ui:screen"), None);
        assert_eq!(parse_request("x1 ui:screen"), None);
        assert_eq!(parse_request("../1 hang_ui"), None);
        let dir = tempfile::tempdir().unwrap();
        dispatch(dir.path(), "ui:screen", echo_ui);
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }

    #[test]
    fn a_request_is_taken_once_and_leftovers_are_removed() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("debug_request.txt"), "3 hang_ui\n").unwrap();
        assert_eq!(take_request(dir.path()).as_deref(), Some("3 hang_ui\n"));
        assert_eq!(take_request(dir.path()), None);
        for name in [
            "debug_request.taking",
            "debug_answer_9.txt",
            "debug_dead",
            "debug_request.txt",
            "debug_ui_timeout_ms.txt",
            "debug_proc",
        ] {
            std::fs::write(dir.path().join(name), "x").unwrap();
        }
        remove_leftovers(dir.path());
        // What the harness set as a number for a shortened wait stays, and so does a request that waits.
        let mut left: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        left.sort();
        assert_eq!(
            left,
            ["debug_proc", "debug_request.txt", "debug_ui_timeout_ms.txt"]
        );
    }

    #[test]
    fn the_process_is_named_in_a_file() {
        let dir = tempfile::tempdir().unwrap();
        write_process_mark(dir.path());
        let name = std::fs::read_to_string(dir.path().join("debug_proc")).unwrap();
        let (pid, nanos) = name.trim_end().split_once('-').unwrap();
        assert_eq!(pid, std::process::id().to_string());
        assert!(nanos.bytes().all(|b| b.is_ascii_digit()), "{name}");
        assert!(name.ends_with('\n'));
        // No temp file is left behind.
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    }
}
