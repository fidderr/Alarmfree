//! Readiness signals for the devtest harness, so a scenario waits for what it needs instead of
//! sleeping. Only in a debug build with the `devtest` feature (and in tests); nothing of it is in a
//! build for users.
//!
//! * first frame drawn: `frames` counts the frames the renderer finished, `first_frame` is 1 from
//!   the first one.
//! * which sheet or panel is open, which screen: the keys of [`state`].
//! * the keyboard (`keyboard=up|down`), the window size (`win=<w>x<h>`) and how often it changed
//!   (`resizes`): Android resizes the window about a second after the first frame, and `stable`
//!   holds when the first frame is drawn and the window did not change for [`STABLE_MS`].
//! * `input_seen`: how many times Android told the app that input is waiting (real touches and
//!   keys; a tap made inside the app does not count). Only on a phone or emulator.
//! * layout settled, animations finished: the renderer drew no frame for a moment. Slint draws
//!   only while something changes or moves (a sliding panel, a fading toast, a pulsing ring), so
//!   an idle window is a settled one. On a screen that is drawn all the time the texts have to
//!   stand where they stood on the poll before.
//!
//! `ui:ready` answers [`state`] at once. `ui:await:<ms>:<cond>;<cond>` answers when every
//! condition holds and the layout is settled, or when the time is up, from the app itself (one adb
//! round trip, no polling from the host). A condition is `key=value` for a key of the state
//! (`key!=value`, `key>N` for a number above N), `text=<exact>`, `textc=<part>`, `gone=<exact>` or
//! `moved` (a frame was drawn since the last await ended) or `stable`. A `~` in front makes a
//! condition soft: it is waited for 400 ms and then let go. A key that is not in [`KEYS`] is an
//! error at once (`ERR unknown key <key>`), not a wait to the time limit.

// Headless tests only run the parts that need no renderer; the rest is for the device.
#![cfg_attr(test, allow(dead_code))]

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use slint::ComponentHandle;

use super::{debug_elements, Insets, MainWindow, Run, Thm};

/// The words of [`state`], the keys a condition may name.
const KEYS: [&str; 16] = [
    "frames",
    "first_frame",
    "screen",
    "home_sheet",
    "panel",
    "editor_sheet",
    "theme_sheet",
    "ringing",
    "toast",
    "ed_preview",
    "scan_state",
    "run_active",
    "keyboard",
    "win",
    "resizes",
    "input_seen",
];
/// Bottom inset from which the window counts as covered by the keyboard (the bars alone are less).
const KEYBOARD_MIN_PX: f32 = 400.0;
/// `stable`: no change of the window size for this long after the first frame.
const STABLE_MS: u64 = 1200;

static FRAMES: AtomicU64 = AtomicU64::new(0);
static RESIZES: AtomicU64 = AtomicU64::new(0);
/// Milliseconds since [`now_ms`] was first asked when the first frame was drawn or the window size last changed.
static CHANGED_MS: AtomicU64 = AtomicU64::new(0);
/// The window size at the last frame, width in the high half.
static LAST_SIZE: AtomicU64 = AtomicU64::new(0);
/// How often Android said that input is waiting.
static INPUT_SEEN: AtomicU64 = AtomicU64::new(0);
/// The frame count when the last `await` ended: what the window drew since then is "moved".
static MARK: AtomicU64 = AtomicU64::new(0);
/// A condition with `~` in front gives up quietly after this long: a touch that changes nothing.
const SOFT_MS: u128 = 400;
/// A screen that is drawn and moves all the time counts as settled this long after its conditions hold.
const ANIMATING_MS: Duration = Duration::from_millis(500);

pub(super) fn now_ms() -> u64 {
    static START: OnceLock<Instant> = OnceLock::new();
    START.get_or_init(Instant::now).elapsed().as_millis() as u64
}

/// Count the frames the renderer finishes, and the changes of the window size between them.
/// Without a renderer that tells (a test window) it stays 0.
pub fn install(ui: &MainWindow) {
    let weak = ui.as_weak();
    let _ = ui.window().set_rendering_notifier(move |state, _| {
        if !matches!(state, slint::RenderingState::AfterRendering) {
            return;
        }
        let first = FRAMES.fetch_add(1, Ordering::Relaxed) == 0;
        let Some(ui) = weak.upgrade() else {
            return;
        };
        let size = ui.window().size();
        let packed = (u64::from(size.width) << 32) | u64::from(size.height);
        if LAST_SIZE.swap(packed, Ordering::Relaxed) != packed || first {
            if !first {
                RESIZES.fetch_add(1, Ordering::Relaxed);
            }
            CHANGED_MS.store(now_ms(), Ordering::Relaxed);
        }
    });
}

/// Android handed the app input (called from the platform's event listener).
pub fn note_input() {
    trace!("android input");
    INPUT_SEEN.fetch_add(1, Ordering::Relaxed);
}

fn stable() -> bool {
    frames() > 0 && now_ms().saturating_sub(CHANGED_MS.load(Ordering::Relaxed)) >= STABLE_MS
}

pub fn frames() -> u64 {
    FRAMES.load(Ordering::Relaxed)
}

/// What is open and drawn, as `key=value` words.
pub fn state(ui: &MainWindow) -> String {
    let frames = frames();
    format!(
        "frames={frames} first_frame={} screen={} home_sheet={:?} panel={} editor_sheet={:?} theme_sheet={} ringing={} toast={} ed_preview={} scan_state={} run_active={} keyboard={} win={}x{} resizes={} input_seen={}",
        u8::from(frames > 0),
        ui.get_screen(),
        ui.get_home_sheet(),
        ui.get_panel_open(),
        ui.get_ed_sheet(),
        ui.global::<Thm>().get_sheet_open(),
        ui.get_ringing(),
        ui.get_toast_visible(),
        ui.get_ed_preview(),
        ui.global::<Run>().get_scan_state(),
        ui.global::<Run>().get_active(),
        if ui.global::<Insets>().get_bottom() * ui.window().scale_factor() > KEYBOARD_MIN_PX {
            "up"
        } else {
            "down"
        },
        ui.window().size().width,
        ui.window().size().height,
        RESIZES.load(Ordering::Relaxed),
        INPUT_SEEN.load(Ordering::Relaxed),
    )
}

fn value_of<'a>(state: &'a str, key: &str) -> Option<&'a str> {
    state
        .split(' ')
        .find_map(|w| w.strip_prefix(key).and_then(|r| r.strip_prefix('=')))
}

/// Does this one condition hold? `None` for one that is not understood.
fn holds(ui: &MainWindow, state: &str, cond: &str) -> Option<bool> {
    if let Some(text) = cond.strip_prefix("textc=") {
        return Some(debug_elements::has_text(ui, text, false));
    }
    if let Some(text) = cond.strip_prefix("text=") {
        return Some(debug_elements::has_text(ui, text, true));
    }
    if let Some(text) = cond.strip_prefix("gone=") {
        return Some(!debug_elements::has_text(ui, text, true));
    }
    if cond == "moved" {
        return Some(frames() > MARK.load(Ordering::Relaxed));
    }
    if cond == "stable" {
        return Some(stable());
    }
    if let Some((key, not)) = cond.split_once("!=") {
        return Some(value_of(state, key)? != not);
    }
    if let Some((key, n)) = cond.split_once('>') {
        let have = value_of(state, key)?.parse::<u64>().ok()?;
        return Some(have > n.parse::<u64>().ok()?);
    }
    let (key, want) = cond.split_once('=')?;
    Some(value_of(state, key)? == want)
}

/// `Err` with the answer for a condition that can never hold: a key the state does not have, or a
/// number that is none. Such a condition would wait to the time limit and then say nothing about why.
fn check_conditions(conds: &[String]) -> Result<(), String> {
    for cond in conds {
        let cond = cond.strip_prefix('~').unwrap_or(cond);
        if ["textc=", "text=", "gone="]
            .iter()
            .any(|p| cond.starts_with(p))
            || cond == "moved"
            || cond == "stable"
        {
            continue;
        }
        let (key, number) = if let Some((key, _)) = cond.split_once("!=") {
            (key, None)
        } else if let Some((key, n)) = cond.split_once('>') {
            (key, Some(n))
        } else if let Some((key, _)) = cond.split_once('=') {
            (key, None)
        } else {
            return Err(format!("ERR bad condition {cond}"));
        };
        if !KEYS.contains(&key) {
            return Err(format!("ERR unknown key {key}"));
        }
        if number.is_some_and(|n| n.parse::<u64>().is_err()) {
            return Err(format!("ERR bad number in {cond}"));
        }
    }
    Ok(())
}

thread_local! {
    static AWAITS: RefCell<Vec<Rc<slint::Timer>>> = const { RefCell::new(Vec::new()) };
}

/// Answer `reply` once the conditions of `spec` (`<ms>:<cond>;<cond>`) hold and the layout is
/// settled, or when the time is up, with what was missing.
pub fn start_await(
    ui: slint::Weak<MainWindow>,
    spec: &str,
    reply: impl FnOnce(String) + Send + 'static,
) {
    let (ms, conds) = spec.split_once(':').unwrap_or((spec, ""));
    let limit = Duration::from_millis(ms.parse().unwrap_or(5000));
    let conds: Vec<String> = conds
        .split(';')
        .filter(|c| !c.is_empty())
        .map(str::to_owned)
        .collect();
    if let Err(answer) = check_conditions(&conds) {
        reply(answer);
        return;
    }
    let started = Instant::now();
    let timer = Rc::new(slint::Timer::default());
    let mut reply = Some(reply);
    let mut last = frames();
    let mut last_print = 0u64;
    let mut met_since: Option<Instant> = None;
    let mut still = 0;
    let handle = timer.clone();
    timer.start(
        slint::TimerMode::Repeated,
        Duration::from_millis(30),
        move || {
            let Some(ui) = ui.upgrade() else {
                handle.stop();
                if let Some(reply) = reply.take() {
                    reply("ERR no window".to_owned());
                }
                return;
            };
            let now = state(&ui);
            let waited = started.elapsed().as_millis();
            let unmet: Vec<&str> = conds
                .iter()
                .filter(|c| match c.strip_prefix('~') {
                    Some(soft) => waited < SOFT_MS && holds(&ui, &now, soft) != Some(true),
                    None => holds(&ui, &now, c) != Some(true),
                })
                .map(String::as_str)
                .collect();
            // Slint draws only when something changed or moves: no frame for a while is a window
            // that holds still.
            let drawn = frames();
            if drawn == last {
                still += 1;
            } else {
                last = drawn;
                // Drawn all the time (a ring screen's backdrop): then the texts have to hold still.
                let print = debug_elements::fingerprint(&ui);
                if print == last_print {
                    still += 1;
                } else {
                    still = 0;
                    last_print = print;
                }
            }
            if unmet.is_empty() {
                met_since.get_or_insert_with(Instant::now);
            } else {
                met_since = None;
            }
            // A screen that never holds still (the ring screen's backdrop and clock) is as ready as
            // it will get a moment after what was waited for is true.
            let animating = met_since.is_some_and(|at| at.elapsed() >= ANIMATING_MS);
            let answer = if unmet.is_empty() && still >= 2 {
                format!("OK {}ms", started.elapsed().as_millis())
            } else if animating {
                format!("OK {}ms (still moving)", started.elapsed().as_millis())
            } else if started.elapsed() >= limit {
                let why = if unmet.is_empty() {
                    "frames kept being drawn: something keeps moving".to_owned()
                } else {
                    format!("not met: {}", unmet.join("; "))
                };
                format!(
                    "TIMEOUT after {}ms, {why}. Now: {now}",
                    started.elapsed().as_millis()
                )
            } else {
                return;
            };
            handle.stop();
            MARK.store(frames(), Ordering::Relaxed);
            if let Some(reply) = reply.take() {
                reply(answer);
            }
        },
    );
    AWAITS.with(|all| {
        let mut all = all.borrow_mut();
        all.retain(|t| t.running());
        all.push(timer);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conditions_read_the_state_line() {
        let s = "frames=7 first_frame=1 screen=3 home_sheet=None panel=false";
        assert_eq!(value_of(s, "screen"), Some("3"));
        assert_eq!(value_of(s, "home_sheet"), Some("None"));
        assert_eq!(value_of(s, "nothing"), None);
        // `frames` must not be found inside `first_frame`'s neighbours.
        assert_eq!(value_of(s, "first"), None);
    }

    fn conds(spec: &str) -> Vec<String> {
        spec.split(';').map(str::to_owned).collect()
    }

    #[test]
    fn an_unknown_key_is_an_error_at_once() {
        assert_eq!(
            check_conditions(&conds("scren=1")),
            Err("ERR unknown key scren".to_owned())
        );
        assert_eq!(
            check_conditions(&conds("screen=1;~nope!=3")),
            Err("ERR unknown key nope".to_owned())
        );
        assert_eq!(
            check_conditions(&conds("frames>x")),
            Err("ERR bad number in frames>x".to_owned())
        );
        assert_eq!(
            check_conditions(&conds("frames")),
            Err("ERR bad condition frames".to_owned())
        );
        // The same answer reaches the one who asked, without a window and without waiting.
        let answers = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let seen = answers.clone();
        start_await(slint::Weak::default(), "30000:nosuch=1", move |a| {
            seen.lock().unwrap().push(a);
        });
        assert_eq!(*answers.lock().unwrap(), ["ERR unknown key nosuch"]);
    }

    #[test]
    fn every_condition_a_scenario_may_use_is_accepted() {
        let all = [
            "first_frame=1",
            "screen!=0",
            "home_sheet=None",
            "panel=true",
            "editor_sheet!=None",
            "theme_sheet=false",
            "ringing=true",
            "toast=false",
            "ed_preview=false",
            "scan_state>0",
            "run_active=true",
            "keyboard=up",
            "win=1080x2400",
            "resizes>0",
            "input_seen>3",
            "frames>10",
            "text=Save",
            "textc=Sa",
            "gone=Save",
            "moved",
            "stable",
            "~moved",
            "~text=a>b",
        ]
        .join(";");
        assert_eq!(check_conditions(&conds(&all)), Ok(()));
        assert_eq!(check_conditions(&[]), Ok(()));
    }

    #[test]
    fn the_state_line_has_only_known_keys() {
        thread_local!(static INIT: () = i_slint_backend_testing::init_no_event_loop());
        INIT.with(|_| {});
        let ui = MainWindow::new().unwrap();
        let line = state(&ui);
        let keys: Vec<&str> = line
            .split(' ')
            .map(|w| w.split_once('=').expect("key=value").0)
            .collect();
        assert_eq!(keys, KEYS, "{line}");
        assert_eq!(value_of(&line, "keyboard"), Some("down"));
        assert!(value_of(&line, "win").is_some_and(|w| w.contains('x')));
    }
}
