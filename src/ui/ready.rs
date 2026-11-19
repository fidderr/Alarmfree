//! Readiness signals for the devtest harness, so a scenario waits for what it needs instead of
//! sleeping. Only in a debug build with the `devtest` feature (and in tests); nothing of it is in a
//! build for users.
//!
//! * first frame drawn: `frames` counts the frames the renderer finished, `first_frame` is 1 from
//!   the first one.
//! * which sheet or panel is open, which screen: the keys of [`state`].
//! * layout settled, animations finished: the renderer drew no frame for a moment. Slint draws
//!   only while something changes or moves (a sliding panel, a fading toast, a pulsing ring), so
//!   an idle window is a settled one, and reading it costs no walk through the element tree.
//!
//! `ui:ready` answers [`state`] at once. `ui:await:<ms>:<cond>;<cond>` answers when every
//! condition holds and the layout is settled, or when the time is up, from the app itself (one adb
//! round trip, no polling from the host). A condition is `key=value` for a key of the state
//! (`key!=value`, `key>N` for a number above N), `text=<exact>`, `textc=<part>`, `gone=<exact>` or
//! `moved` (a frame was drawn since the last await ended). A `~` in front makes a condition soft: it
//! is waited for 400 ms and then let go.

// Headless tests only run the parts that need no renderer; the rest is for the device.
#![cfg_attr(test, allow(dead_code))]

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use slint::ComponentHandle;

use super::{debug_elements, MainWindow, Run, Thm};

static FRAMES: AtomicU64 = AtomicU64::new(0);
/// The frame count when the last `await` ended: what the window drew since then is "moved".
static MARK: AtomicU64 = AtomicU64::new(0);
/// A condition with `~` in front gives up quietly after this long: a touch that changes nothing.
const SOFT_MS: u128 = 400;

/// Count the frames the renderer finishes. Without a renderer that tells (a test window) it stays 0.
pub fn install(ui: &MainWindow) {
    let _ = ui.window().set_rendering_notifier(|state, _| {
        if matches!(state, slint::RenderingState::AfterRendering) {
            FRAMES.fetch_add(1, Ordering::Relaxed);
        }
    });
}

pub fn frames() -> u64 {
    FRAMES.load(Ordering::Relaxed)
}

/// What is open and drawn, as `key=value` words.
pub fn state(ui: &MainWindow) -> String {
    let frames = frames();
    format!(
        "frames={frames} first_frame={} screen={} home_sheet={:?} panel={} editor_sheet={:?} theme_sheet={} ringing={} toast={} ed_preview={} scan_state={} run_active={}",
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
    let started = Instant::now();
    let timer = Rc::new(slint::Timer::default());
    let mut reply = Some(reply);
    let mut last = frames();
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
                still = 0;
                last = drawn;
            }
            let answer = if unmet.is_empty() && still >= 2 {
                format!("OK {}ms", started.elapsed().as_millis())
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
}
