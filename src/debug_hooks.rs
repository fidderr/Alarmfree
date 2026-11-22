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
//! The app knows from the life cycle events of the activity whether its UI can run commands right
//! now: it has a window and is not paused ([`window`], [`paused`], [`ui_can_run`]). While it cannot
//! (an Android screen or one of the SDK is in front, for example the unlock screen), the UI thread
//! runs nothing, so waiting for it would end in no answer and a closed channel:
//!   a command is answered at once with `ERR the app has no window in front`;
//!   an `await` is held and handed to the UI when it can run again, with the time left; when its own
//!   time passes first the answer is `TIMEOUT after <n>ms, the app had no window in front`.
//! A UI thread that has a window and still runs nothing is a stuck one: no answer, as before.
//!
//! `<files>/debug_<name>.txt` holds a number to use instead of a wait that takes too long
//! for a test, see [`crate::platform::shortened`].

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use mobile_sentinel::InstanceId;

use crate::platform::alarm_kit_optional;

static HANG_UI: AtomicBool = AtomicBool::new(false);

const HAS_WINDOW: u8 = 1;
const IS_PAUSED: u8 = 2;
/// What the life cycle events say; no window until the platform made one.
static LIFE: AtomicU8 = AtomicU8::new(0);
/// A supervised command that nobody answered is given up on this long after its deadline.
const SUPERVISE_GRACE: Duration = Duration::from_secs(35);

/// The platform made the window (`true`) or took it away.
pub fn window(has: bool) {
    if has {
        LIFE.fetch_or(HAS_WINDOW, Ordering::SeqCst);
    } else {
        LIFE.fetch_and(!HAS_WINDOW, Ordering::SeqCst);
    }
}

/// The activity paused or stopped (`true`) or resumed.
pub fn paused(is: bool) {
    if is {
        LIFE.fetch_or(IS_PAUSED, Ordering::SeqCst);
    } else {
        LIFE.fetch_and(!IS_PAUSED, Ordering::SeqCst);
    }
}

/// Can the UI thread run a command now?
pub fn ui_can_run() -> bool {
    LIFE.load(Ordering::SeqCst) == HAS_WINDOW
}

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
            dispatch_gated(&dir, &text, run_ui, ui_can_run);
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

/// [`dispatch_gated`] for a UI that can always run.
#[cfg(test)]
fn dispatch(dir: &Path, text: &str, run_ui: fn(String, Reply)) {
    dispatch_gated(dir, text, run_ui, || true);
}

/// The time of `await:<ms>:<conditions>` and the conditions; the time as [`ready`](crate::ui) reads it.
fn parse_await(ui: &str) -> Option<(u64, &str)> {
    let spec = ui.strip_prefix("await:")?;
    let (ms, conds) = spec.split_once(':').unwrap_or((spec, ""));
    Some((ms.parse().unwrap_or(5000), conds))
}

/// Answer one request: `ui:` commands through `run_ui` (the answer comes when the UI thread has
/// run it, or at once when it cannot run, see the top of this file), the others at once.
fn dispatch_gated(dir: &Path, text: &str, run_ui: fn(String, Reply), can_run: fn() -> bool) {
    let Some((id, command)) = parse_request(text) else {
        log::warn!("[AlarmFree] DEBUG request without an id: {text}");
        return;
    };
    let Some(ui) = command.strip_prefix("ui:") else {
        write_answer(dir, id, &run_action(command));
        return;
    };
    let started = Instant::now();
    let awaiting = parse_await(ui);
    let pending = Pending {
        dir: dir.to_path_buf(),
        id: id.to_owned(),
        done: false,
    };
    let limit = Duration::from_millis(awaiting.map_or(0, |(ms, _)| ms));
    let mut command = ui.to_owned();
    if !can_run() {
        let Some((ms, conds)) = awaiting else {
            pending.answer("ERR the app has no window in front");
            return;
        };
        while !can_run() {
            if started.elapsed() >= limit {
                pending.answer(&format!(
                    "TIMEOUT after {ms}ms, the app had no window in front"
                ));
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let left = limit.saturating_sub(started.elapsed()).as_millis().max(1);
        command = format!("await:{left}:{conds}");
    }
    let mine = Arc::new(Mutex::new(Some(pending)));
    let reply = Dropped(mine.clone());
    run_ui(
        command,
        Box::new(move |answer| {
            if let Some(pending) = reply.0.lock().unwrap().take() {
                pending.answer(&answer);
            }
        }),
    );
    // The window can go while the command waits for the UI thread: then nobody answers it.
    let kind = awaiting.map(|(ms, _)| ms);
    std::thread::spawn(move || supervise(&mine, started + limit, kind, can_run));
}

/// Held by the reply: when the reply is dropped without being called, the dropped [`Pending`] answers.
struct Dropped(Arc<Mutex<Option<Pending>>>);

impl Drop for Dropped {
    fn drop(&mut self) {
        self.0.lock().unwrap().take();
    }
}

/// Watch a command that was handed to the UI thread until it is answered. When its time is over and
/// the UI cannot run, answer for it. A UI that has a window and does not answer is left alone (no answer, as before).
fn supervise(
    arc: &Arc<Mutex<Option<Pending>>>,
    deadline: Instant,
    await_ms: Option<u64>,
    can_run: fn() -> bool,
) {
    let give_up = deadline + SUPERVISE_GRACE;
    loop {
        std::thread::sleep(Duration::from_millis(10));
        let mut slot = arc.lock().unwrap();
        if slot.is_none() {
            return;
        }
        if !can_run() && Instant::now() >= deadline {
            if let Some(pending) = slot.take() {
                pending.answer(&match await_ms {
                    Some(ms) => format!("TIMEOUT after {ms}ms, the app had no window in front"),
                    None => "ERR the app has no window in front".to_owned(),
                });
            }
            return;
        }
        if Instant::now() >= give_up {
            return;
        }
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

    /// A UI thread that must not be asked: for a state in which the command may not be handed over.
    fn never_asked(_: String, _: Reply) {
        panic!("the command was handed to a UI that cannot run");
    }

    fn wait_for_answer(dir: &Path, id: &str, secs: u64) -> Option<String> {
        let end = Instant::now() + Duration::from_secs(secs);
        while Instant::now() < end {
            if let Some(a) = answer(dir, id) {
                return Some(a);
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        None
    }

    #[test]
    fn the_life_cycle_decides_whether_the_ui_can_run() {
        // The only test that touches the real state.
        assert!(!ui_can_run(), "no window before the platform made one");
        window(true);
        assert!(ui_can_run());
        paused(true);
        assert!(!ui_can_run());
        paused(false);
        assert!(ui_can_run());
        window(false);
        assert!(!ui_can_run());
        window(true);
        window(false);
    }

    #[test]
    fn a_command_while_the_ui_cannot_run_is_answered_at_once() {
        let dir = tempfile::tempdir().unwrap();
        let t = Instant::now();
        dispatch_gated(dir.path(), "21-1 ui:screen", never_asked, || false);
        assert_eq!(
            answer(dir.path(), "21-1").as_deref(),
            Some("ERR the app has no window in front")
        );
        assert!(t.elapsed() < Duration::from_millis(500));
        // The same for a command that acts is not affected: the app itself can run it.
        dispatch_gated(dir.path(), "21-2 hang_ui", never_asked, || false);
        HANG_UI.store(false, Ordering::SeqCst);
        assert_eq!(answer(dir.path(), "21-2").as_deref(), Some("OK"));
    }

    #[test]
    fn an_await_while_the_ui_cannot_run_times_out_in_its_own_time() {
        let dir = tempfile::tempdir().unwrap();
        let t = Instant::now();
        dispatch_gated(dir.path(), "22-1 ui:await:200:~moved", never_asked, || {
            false
        });
        let took = t.elapsed();
        assert_eq!(
            answer(dir.path(), "22-1").as_deref(),
            Some("TIMEOUT after 200ms, the app had no window in front")
        );
        assert!(took >= Duration::from_millis(190) && took < Duration::from_secs(2));
    }

    #[test]
    fn an_await_is_run_when_the_ui_comes_back_in_time() {
        static BACK: AtomicBool = AtomicBool::new(false);
        let dir = tempfile::tempdir().unwrap();
        std::thread::spawn(|| {
            std::thread::sleep(Duration::from_millis(150));
            BACK.store(true, Ordering::SeqCst);
        });
        dispatch_gated(
            dir.path(),
            "23-1 ui:await:5000:screen=home",
            echo_ui,
            || BACK.load(Ordering::SeqCst),
        );
        let got = answer(dir.path(), "23-1").unwrap();
        // Run with the time that was left, and the same conditions.
        let left: u64 = got
            .strip_prefix("ui says await:")
            .and_then(|r| r.strip_suffix(":screen=home"))
            .unwrap_or_else(|| panic!("{got}"))
            .parse()
            .unwrap();
        assert!(left > 0 && left < 5000 - 100, "{left}");
    }

    #[test]
    fn a_stuck_ui_with_a_window_gives_no_answer() {
        fn hold(_: String, reply: Reply) {
            std::mem::forget(reply);
        }
        let dir = tempfile::tempdir().unwrap();
        dispatch_gated(dir.path(), "24-1 ui:screen", hold, || true);
        dispatch_gated(dir.path(), "24-2 ui:await:100:~moved", hold, || true);
        std::thread::sleep(Duration::from_millis(500));
        assert_eq!(answer(dir.path(), "24-1"), None);
        assert_eq!(answer(dir.path(), "24-2"), None);
    }

    #[test]
    fn a_window_that_goes_while_a_command_waits_for_the_ui_is_answered() {
        static UP: AtomicBool = AtomicBool::new(true);
        static HELD: Mutex<Vec<Reply>> = Mutex::new(Vec::new());
        fn hold(_: String, reply: Reply) {
            HELD.lock().unwrap().push(reply);
        }
        let dir = tempfile::tempdir().unwrap();
        dispatch_gated(dir.path(), "25-1 ui:screen", hold, || {
            UP.load(Ordering::SeqCst)
        });
        dispatch_gated(dir.path(), "25-2 ui:await:300:~moved", hold, || {
            UP.load(Ordering::SeqCst)
        });
        assert_eq!(answer(dir.path(), "25-1"), None);
        UP.store(false, Ordering::SeqCst);
        assert_eq!(
            wait_for_answer(dir.path(), "25-1", 3).as_deref(),
            Some("ERR the app has no window in front")
        );
        assert_eq!(
            wait_for_answer(dir.path(), "25-2", 3).as_deref(),
            Some("TIMEOUT after 300ms, the app had no window in front")
        );
        // A late answer from the UI does not replace an answer that was given.
        for reply in HELD.lock().unwrap().drain(..) {
            reply("late".to_owned());
        }
        assert_ne!(answer(dir.path(), "25-1").as_deref(), Some("late"));
        assert_ne!(answer(dir.path(), "25-2").as_deref(), Some("late"));
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
