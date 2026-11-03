// SECURITY: Dioxus RSX string interpolation ("{var}") auto-escapes HTML entities.
// No manual escaping needed for user-provided strings rendered via interpolation.
// No `dangerous_inner_html` is used anywhere in this codebase.

use dioxus::prelude::*;
use std::collections::HashMap;

use crate::back::{BackStack, BACK_PRESSED};
use crate::i18n;
use crate::models::AppSettings;
use crate::screens::AlarmConfig;
use crate::screens::FirstRun;
use crate::screens::History;
use crate::screens::Home;
use crate::screens::Practice;
use crate::screens::Reliability;
use crate::screens::Settings;

/// Type alias for the translations signal used across the app.
pub type Translations = Signal<HashMap<String, String>>;

/// Get a translation value by key. Returns the key itself if not found.
pub fn t(translations: &Translations, key: &str) -> String {
    translations
        .read()
        .get(key)
        .cloned()
        .unwrap_or_else(|| key.to_string())
}

/// Navigation routes for the app.
#[derive(Clone, Routable, Debug, PartialEq)]
pub enum Route {
    #[layout(NavWatcher)]
    #[route("/")]
    Home {},
    #[route("/alarm/:id")]
    AlarmConfig { id: String },
    #[route("/practice/:id")]
    Practice { id: String },
    #[route("/history")]
    History {},
    #[route("/settings")]
    Settings {},
    #[route("/reliability")]
    Reliability {},
    #[route("/welcome")]
    FirstRun {},
}

/// Router-scoped layout that performs a one-shot navigation requested via
/// the `nav_intent` context signal, and leaves a screen on Back.
///
/// The firing screen (`AlarmFiring`) is rendered *outside* the `Router`
/// (see [`App`]), so it cannot call `use_navigator()` itself. Instead it
/// sets `nav_intent` (e.g. to `Route::History` after a successful
/// dismiss). When firing ends and the `Router` remounts, this layout's
/// effect picks up the intent, navigates, and clears it.
#[component]
fn NavWatcher() -> Element {
    let mut nav_intent = use_context::<Signal<Option<Route>>>();
    let mut leave_screen = use_context::<Signal<u32>>();
    let settings = use_context::<Signal<AppSettings>>();
    let wide = use_context::<WideScreen>();
    let translations = use_context::<Translations>();
    let nav = use_navigator();
    let route = use_route::<Route>();
    let split = wide.0() && matches!(route, Route::Home {} | Route::AlarmConfig { .. });
    let only_the_list = route == (Route::Home {});
    use_effect(move || {
        if let Some(route) = nav_intent() {
            nav_intent.set(None);
            nav.replace(route);
        }
    });
    // Once, before anything else.
    use_hook(|| {
        if !settings.peek().prefs.setup_done {
            nav.replace(Route::FirstRun {});
        }
    });
    let mut current = use_signal(|| route.clone());
    if *current.peek() != route {
        current.set(route);
    }
    use_effect(move || {
        if leave_screen() == 0 {
            return;
        }
        leave_screen.set(0);
        match &*current.peek() {
            Route::Home {} => mobile_sentinel::foregrounding::move_to_back(),
            // The main screens do not stack up.
            Route::History {} | Route::Settings {} => {
                nav.replace(Route::Home {});
            }
            _ if nav.can_go_back() => nav.go_back(),
            _ => {
                nav.replace(Route::Home {});
            }
        }
    });
    if split {
        return rsx! {
            div { class: "split",
                div { class: "split-list", Home {} }
                div { class: "split-detail",
                    if only_the_list {
                        p { class: "split-empty", {t(&translations, "home.pick")} }
                    } else {
                        Outlet::<Route> {}
                    }
                }
            }
        };
    }
    rsx! { Outlet::<Route> {} }
}

/// The screen is wide enough for the list and an alarm next to each other.
#[derive(Clone, Copy)]
struct WideScreen(Signal<bool>);

/// From this width in CSS pixels on: a tablet, or a foldable when unfolded.
const SPLIT_FROM_WIDTH: i32 = 840;

/// Shared alarm data stored in global state.
#[derive(Clone, PartialEq, Debug, serde::Serialize, serde::Deserialize)]
pub struct AlarmData {
    pub id: String,
    pub time_hour: u8,
    pub time_minute: u8,
    pub label: String,
    pub days: [bool; 7],
    /// Date (`YYYY-MM-DD`) of a one-time alarm; `None` for weekday alarms and
    /// for alarms saved before this field existed.
    #[serde(default)]
    pub date: Option<String>,
    /// Skip every occurrence at or before this instant (unix ms): one skipped
    /// occurrence of a repeating alarm. In the past it has no effect.
    #[serde(default)]
    pub skip_until_ms: Option<i64>,
    /// Seconds the sound takes to fade in (0 = off).
    #[serde(default)]
    pub fade_in_secs: u16,
    /// Stop by itself after ringing this long with nobody doing anything, in
    /// seconds. `None` rings until it is dealt with.
    #[serde(default = "default_ring_limit_secs")]
    pub ring_limit_secs: Option<u32>,
    /// Position in the home list once the user has dragged alarms around (see
    /// [`crate::ordering`]); 0 = not arranged, listed by time.
    #[serde(default)]
    pub order: i64,
    pub enabled: bool,
    pub sound: String,
    pub sound_mode: usize,                 // 0=Sound+Vib, 1=Sound, 2=Vib
    pub challenges: Vec<(String, String)>, // (type, settings), see ChallengeSetup
    pub snooze_count: u8,
    pub snooze_interval: u8,
    /// Shown under the label while it rings ("Take medication").
    #[serde(default)]
    pub note: String,
    /// Vibration pattern, see [`crate::platform::VIBRATION_PATTERNS`]. Whether it
    /// vibrates at all is `sound_mode`.
    #[serde(default)]
    pub vibration: u8,
    /// The alarm volume while it rings is at least this; `None` is the general
    /// minimum from the settings.
    #[serde(default)]
    pub volume_percent: Option<u8>,
    /// Do the challenges in a different order each time.
    #[serde(default)]
    pub random_order: bool,
    /// This long after it was turned off, confirm being up or it rings again (0 = off).
    #[serde(default)]
    pub wake_check_minutes: u8,
    /// The screen goes to full brightness while it rings.
    #[serde(default)]
    pub max_brightness: bool,
    /// Only vibrate this long before the sound starts. Needs both on (`sound_mode` 0).
    #[serde(default)]
    pub sound_delay_secs: u16,
    /// Nothing rings until then, for a holiday. Separate from a skipped ring, so
    /// undoing one leaves the other.
    #[serde(default)]
    pub pause_until_ms: Option<i64>,
    /// Gone instead of switched off when over: quick alarms, wake-up checks.
    #[serde(default)]
    pub delete_after_ring: bool,
    /// Not in the list: wake-up checks and test alarms.
    #[serde(default)]
    pub hidden: bool,
}

fn default_ring_limit_secs() -> Option<u32> {
    Some(mobile_sentinel::context::schema::DEFAULT_RING_LIMIT_SECS)
}

/// In the binary and as a plain element: no file to load and no `eval` to wait for, so a
/// page that has trouble still shows what it says properly.
const STYLESHEET: &str = include_str!("../assets/style.css");

/// Says when the window may have changed. Edge to edge, a keyboard changes nothing the page
/// can see, so it also looks for as long as something can be typed in, and keeps what is
/// being typed in out from under the keyboard.
///
/// And it switches the page to applying changes at once. It does so in an animation frame
/// otherwise, and nothing in the UI runs until the last change is applied: a WebView that is
/// not drawing, as after another screen closed, has no frames until it is touched, and until
/// then every task of the UI stands still, also what turns an alarm off after a challenge.
/// Dioxus's mode for a page nobody sees does without them.
const WINDOW_WATCH: &str = "\
    window.interpreter.headless = true; \
    const tell = () => dioxus.send(innerWidth); \
    addEventListener('resize', tell); \
    document.addEventListener('visibilitychange', tell); \
    matchMedia('(prefers-color-scheme: dark)').addEventListener('change', tell); \
    const typing = () => document.activeElement?.matches('input:not([type=range]):not([type=date]), textarea'); \
    const showField = () => { \
        const root = document.querySelector('.app-root'); \
        if (!root || !typing()) return; \
        const free = innerHeight - parseFloat(getComputedStyle(root).getPropertyValue('--safe-bottom')); \
        const box = document.activeElement.getBoundingClientRect(); \
        if (box.bottom > free || box.top < 0) document.activeElement.scrollIntoView({ block: 'center' }); \
    }; \
    let watching = 0; \
    document.addEventListener('focusin', () => { \
        if (watching) return; \
        let quiet = 0; \
        watching = setInterval(() => { \
            tell(); \
            showField(); \
            quiet = typing() ? 0 : quiet + 1; \
            if (quiet > 5) { clearInterval(watching); watching = 0; } \
        }, 200); \
    }); \
    tell();";

/// History is kept this long.
const HISTORY_KEEP_DAYS: u32 = 5 * 365;

/// Root App component with global state context providers.
#[component]
pub fn App() -> Element {
    // Initialize platform: logger, FiringSink, AlarmKit, alarm rearm. First,
    // so the alarm list below already reflects one-time alarms it switched off.
    use_hook(crate::platform::init_platform);

    // Global state signals provided via context.
    let settings: Signal<AppSettings> = use_signal(crate::persistence::load_settings);
    let mut alarms = use_signal(crate::persistence::load_alarms);
    let translations: Translations = use_signal(|| i18n::load_translations(settings.peek().locale));

    use_context_provider(|| settings);
    use_context_provider(|| alarms);
    use_context_provider(|| translations);

    use_hook(|| {
        crate::feedback::set_enabled(settings.peek().prefs.touch_feedback);
        crate::history_persistence::delete_history(Some(HISTORY_KEEP_DAYS));
    });

    // Back: whatever is on top takes it, else the screen is left. While an alarm rings
    // there is nothing to go back to, on any Android version.
    let back_stack = use_context_provider(BackStack::default);
    let mut leave_screen: Signal<u32> = use_signal(|| 0);
    use_context_provider(|| leave_screen);
    use_hook(|| mobile_sentinel::foregrounding::on_back(|| BACK_PRESSED.notify_one()));

    // One-shot navigation intent. Set by screens rendered outside the
    // Router (the firing screen) to request a route once the Router
    // remounts. Consumed and cleared by `NavWatcher`.
    let nav_intent: Signal<Option<Route>> = use_signal(|| None);
    use_context_provider(|| nav_intent);

    // Toast notification state
    let mut toast_signal: Signal<Option<crate::components::ToastMessage>> = use_signal(|| None);
    use_context_provider(|| toast_signal);

    // Anything the user must know about what happened while the app was away:
    // a database that had to be reset (or cannot persist) instead of a silently
    // empty alarm list, and alarms that ended without the user.
    use_hook(|| {
        use crate::persistence::DbRecovery;
        let mut notices = Vec::new();
        match crate::persistence::db_recovery() {
            DbRecovery::None => {}
            DbRecovery::Recovered { .. } => notices.push(t(&translations, "db.recovered")),
            DbRecovery::Volatile => notices.push(t(&translations, "db.volatile")),
        }
        let unreadable = crate::persistence::unreadable_alarms();
        if unreadable > 0 {
            notices.push(
                t(&translations, "db.unreadable").replace("{count}", &unreadable.to_string()),
            );
        }
        notices.extend(unseen_outcome_notices(&translations));
        if !notices.is_empty() {
            toast_signal.set(Some(crate::components::ToastMessage::error(
                notices.join(" "),
            )));
        }
    });

    // Firing-state signal — primed from disk on mount, then updated by the
    // single async poll task below, which only writes the signal when the
    // firing instance id changes (a cross-process write by the Recipe state
    // machine).
    let mut firing_id: Signal<Option<String>> = use_signal(crate::firing_state::peek_firing_alarm);

    use_future(move || {
        let back_stack = back_stack.clone();
        async move {
            loop {
                BACK_PRESSED.notified().await;
                if firing_id.peek().is_none() && !back_stack.pop() {
                    leave_screen += 1;
                }
            }
        }
    });

    use_future(move || async move {
        let mut watch = crate::firing_state::FiringWatch::default();
        loop {
            crate::timer::sleep_ms(crate::firing_state::POLL_MS).await;
            if crate::persistence::take_save_failure() {
                toast_signal.set(Some(crate::components::ToastMessage::error(t(
                    &translations,
                    "db.save_failed",
                ))));
            }
            if crate::platform::take_ring_end_news() {
                let mut list = alarms;
                list.set(crate::persistence::load_alarms());
                // Rang until its limit with nobody there: nothing else tells the user.
                let notices = unseen_outcome_notices(&translations);
                if !notices.is_empty() {
                    toast_signal.set(Some(crate::components::ToastMessage::error(
                        notices.join(" "),
                    )));
                }
            }
            // One `stat` per tick; the records are only re-read on change.
            if !watch.should_reread(crate::firing_state::store_marker()) {
                continue;
            }
            let current = crate::firing_state::peek_firing_alarm();
            if current != *firing_id.peek() {
                firing_id.set(current);
            }
        }
    });

    // Proof of life while an alarm rings. If this UI freezes, the beat stops
    // and the `:sentinel` guardian restarts the app (the alarm keeps ringing
    // and the lockdown stays; there is no way out but solving it).
    use_future(move || async move {
        loop {
            crate::timer::sleep_ms(1000).await;
            if firing_id.peek().is_none() {
                continue;
            }
            #[cfg(debug_assertions)]
            crate::debug_hooks::block_if_requested();
            if ui_round_trip().await {
                mobile_sentinel::jobs::ui_heartbeat();
            }
        }
    });

    #[cfg(debug_assertions)]
    use_future(crate::debug_hooks::script_loop);

    // System bars. Android 15+ draws the app edge-to-edge (under the status
    // and navigation bars), so the UI needs to know how far the bars overlap
    // (exposed to CSS as --safe-*), and the bar icons must contrast with our
    // background (dark icons on the light theme).
    let mut wide = use_signal(|| false);
    use_context_provider(|| WideScreen(wide));
    let mut safe_area = use_signal(SafeArea::default);
    let mut system_dark = use_signal(mobile_sentinel::display::system_dark);
    use_effect(move || {
        let light = settings.read().theme.is_light(system_dark());
        mobile_sentinel::display::set_light_system_bars(light);
    });
    let mut read_window = move || {
        let current = SafeArea::query();
        if current != *safe_area.peek() {
            safe_area.set(current);
            // A recreated window can reset the icon colour.
            let light = settings.peek().theme.is_light(*system_dark.peek());
            mobile_sentinel::display::set_light_system_bars(light);
        }
        let dark = mobile_sentinel::display::system_dark();
        if dark != *system_dark.peek() {
            system_dark.set(dark);
        }
    };
    // These only change with the window: a rotation, a fold, coming back from the
    // phone's settings. The page says when; the bars may settle just after the start.
    use_future(move || async move {
        for delay_ms in [0, 300, 1500] {
            crate::timer::sleep_ms(delay_ms).await;
            read_window();
        }
    });
    use_future(move || async move {
        let mut changes = document::eval(WINDOW_WATCH);
        while let Ok(width) = changes.recv::<i32>().await {
            let is_wide = width >= SPLIT_FROM_WIDTH;
            if is_wide != *wide.peek() {
                wide.set(is_wide);
            }
            read_window();
        }
    });

    // Read the signal reactively inside rsx! so Dioxus tracks it.
    let firing_id_val = firing_id.read().clone();

    // Read theme reactively inside rsx! so Dioxus tracks it as a dependency
    let theme_class = settings.read().theme.css_class(system_dark());
    let motion_class = if settings.read().prefs.less_motion {
        "less-motion"
    } else {
        ""
    };
    let accent_class = settings.read().prefs.accent.css_class();
    let safe_style = safe_area.read().css_vars();

    rsx! {
        style { "{STYLESHEET}" }
        div { class: "app-root {theme_class} {accent_class} {motion_class}", style: "{safe_style}",
            if let Some(alarm_id) = firing_id_val {
                // Show alarm firing screen directly, bypassing router
                crate::screens::AlarmFiring { key: "{alarm_id}", id: alarm_id }
            } else {
                Router::<Route> {}
            }

            // Toast notification overlay
            if let Some(msg) = toast_signal.read().clone() {
                crate::components::Toast {
                    key: "{msg.message}",
                    msg: msg,
                    on_dismiss: move |_| { toast_signal.set(None); },
                    on_undo: move |before: Vec<AlarmData>| {
                        alarms.set(before);
                        crate::persistence::save_alarms(&alarms.peek());
                        toast_signal.set(None);
                    },
                }
            }
        }
    }
}

/// What to tell the user about alarms that ended without them, since the last time
/// the app was on screen.
fn unseen_outcome_notices(translations: &Translations) -> Vec<String> {
    use crate::models::AlarmOutcome;
    let unseen = crate::history_persistence::take_unseen_outcomes();
    let mut notices = Vec::new();
    for (outcome, key) in [
        (AlarmOutcome::Reboot, "alarm.ended_by_reboot"),
        (AlarmOutcome::Missed, "alarm.missed"),
        (AlarmOutcome::Limit, "alarm.ended_by_limit"),
    ] {
        let names: Vec<&str> = unseen
            .iter()
            .filter(|(o, _)| *o == outcome)
            .map(|(_, name)| name.as_str())
            .collect();
        if !names.is_empty() {
            notices.push(t(translations, key).replace("{alarm}", &names.join(", ")));
        }
    }
    notices
}

/// A round trip through the page: Rust asks the WebView to answer, the
/// answer comes back. It only succeeds while this async loop, the bridge and
/// the page all work, so it is what a heartbeat must be based on — a bare
/// timer can keep running next to a page that stopped responding.
async fn ui_round_trip() -> bool {
    tokio::time::timeout(std::time::Duration::from_secs(3), async {
        let mut eval = document::eval("dioxus.send(1);");
        eval.recv::<i32>().await.is_ok()
    })
    .await
    .unwrap_or(false)
}

/// System-bar overlap in CSS px, rounded to whole px so tiny float jitter
/// never triggers a re-render.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct SafeArea {
    top: u32,
    bottom: u32,
    left: u32,
    right: u32,
}

impl SafeArea {
    fn query() -> Self {
        mobile_sentinel::display::safe_area_insets()
            .map(|i| Self {
                top: i.top.round() as u32,
                bottom: i.bottom.max(i.keyboard).round() as u32,
                left: i.left.round() as u32,
                right: i.right.round() as u32,
            })
            .unwrap_or_default()
    }

    fn css_vars(&self) -> String {
        format!(
            "--safe-top: {}px; --safe-bottom: {}px; --safe-left: {}px; --safe-right: {}px;",
            self.top, self.bottom, self.left, self.right
        )
    }
}

#[cfg(test)]
mod tests {
    use super::SafeArea;

    #[test]
    fn safe_area_renders_css_variables() {
        let s = SafeArea {
            top: 24,
            bottom: 48,
            left: 0,
            right: 0,
        };
        assert_eq!(
            s.css_vars(),
            "--safe-top: 24px; --safe-bottom: 48px; --safe-left: 0px; --safe-right: 0px;"
        );
        assert_eq!(SafeArea::default().css_vars().matches("0px").count(), 4);
    }
}
