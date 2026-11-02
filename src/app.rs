// SECURITY: Dioxus RSX string interpolation ("{var}") auto-escapes HTML entities.
// No manual escaping needed for user-provided strings rendered via interpolation.
// No `dangerous_inner_html` is used anywhere in this codebase.

use dioxus::prelude::*;
use std::collections::HashMap;

use crate::i18n;
use crate::screens::AlarmConfig;
use crate::screens::AlarmFiring;
use crate::screens::History;
use crate::screens::Home;
use crate::screens::Practice;
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
    #[route("/alarm-firing/:id")]
    AlarmFiring { id: String },
    #[route("/practice/:id")]
    Practice { id: String },
    #[route("/history")]
    History {},
    #[route("/settings")]
    Settings {},
}

/// Router-scoped layout that performs a one-shot navigation requested via
/// the `nav_intent` context signal.
///
/// The firing screen (`AlarmFiring`) is rendered *outside* the `Router`
/// (see [`App`]), so it cannot call `use_navigator()` itself. Instead it
/// sets `nav_intent` (e.g. to `Route::History` after a successful
/// dismiss). When firing ends and the `Router` remounts, this layout's
/// effect picks up the intent, navigates, and clears it.
#[component]
fn NavWatcher() -> Element {
    let mut nav_intent = use_context::<Signal<Option<Route>>>();
    let nav = use_navigator();
    use_effect(move || {
        if let Some(route) = nav_intent() {
            nav_intent.set(None);
            nav.replace(route);
        }
    });
    rsx! { Outlet::<Route> {} }
}

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
    pub challenges: Vec<(String, String)>, // (type, difficulty)
    pub snooze_count: u8,
    pub snooze_interval: u8,
}

fn default_ring_limit_secs() -> Option<u32> {
    Some(mobile_sentinel::context::schema::DEFAULT_RING_LIMIT_SECS)
}

/// Remove an alarm from the list. Returns the custom sound file that is left
/// unused by it, if any: a duplicated alarm shares its sound with the
/// original, so the file may only be deleted when no other alarm uses it.
pub fn remove_alarm(list: &mut Vec<AlarmData>, id: &str) -> Option<String> {
    let removed = list
        .iter()
        .position(|a| a.id == id)
        .map(|i| list.remove(i))?;
    let orphaned = crate::custom_sounds::is_custom_sound(&removed.sound)
        && !list.iter().any(|a| a.sound == removed.sound);
    orphaned.then_some(removed.sound)
}

/// Root App component with global state context providers.
#[component]
pub fn App() -> Element {
    // Load persisted settings (theme + locale).
    let settings = use_hook(crate::persistence::load_settings);

    // Initialize platform: logger, FiringSink, AlarmKit, alarm rearm. First,
    // so the alarm list below already reflects one-time alarms it switched off.
    use_hook(crate::platform::init_sentinel);

    // Global state signals provided via context.
    let theme = use_signal(|| settings.theme);
    let locale = use_signal(|| settings.locale);
    let alarms = use_signal(crate::persistence::load_alarms);
    let translations: Translations = use_signal(|| i18n::load_translations(settings.locale));

    use_context_provider(|| theme);
    use_context_provider(|| locale);
    use_context_provider(|| alarms);
    use_context_provider(|| translations);

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
        notices.extend(unseen_outcome_notices(&translations));
        if !notices.is_empty() {
            toast_signal.set(Some(crate::components::ToastMessage {
                message: notices.join(" "),
                level: crate::components::ToastLevel::Error,
            }));
        }
    });

    // Firing-state signal — primed from disk on mount, then updated by the
    // single async poll task below, which only writes the signal when the
    // firing instance id changes (a cross-process write by the Recipe state
    // machine).
    let mut firing_id: Signal<Option<String>> = use_signal(crate::firing_state::peek_firing_alarm);

    use_future(move || async move {
        let mut watch = crate::firing_state::FiringWatch::default();
        loop {
            crate::timer::sleep_ms(crate::firing_state::POLL_MS).await;
            // One `stat` per tick; the records are only re-read on change.
            if !watch.should_reread(crate::firing_state::store_marker()) {
                continue;
            }
            let current = crate::firing_state::peek_firing_alarm();
            if current != *firing_id.peek() {
                firing_id.set(current);
            }
            if crate::platform::handle_ring_ends() {
                let mut list = alarms;
                list.set(crate::persistence::load_alarms());
                // Rang until its limit with nobody there: nothing else tells the user.
                let notices = unseen_outcome_notices(&translations);
                if !notices.is_empty() {
                    toast_signal.set(Some(crate::components::ToastMessage {
                        message: notices.join(" "),
                        level: crate::components::ToastLevel::Error,
                    }));
                }
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
            crate::platform::debug_block_if_requested();
            if ui_round_trip().await {
                mobile_sentinel::jobs::ui_heartbeat();
            }
        }
    });

    // System bars. Android 15+ draws the app edge-to-edge (under the status
    // and navigation bars), so the UI needs to know how far the bars overlap
    // (exposed to CSS as --safe-*), and the bar icons must contrast with our
    // background (dark icons on the light theme).
    let mut safe_area = use_signal(SafeArea::default);
    use_effect(move || {
        let light = matches!(theme(), crate::models::ThemeMode::Light);
        mobile_sentinel::display::set_light_system_bars(light);
    });
    use_future(move || async move {
        loop {
            // Cheap JNI read; values only change on rotation / window changes.
            let current = SafeArea::query();
            if current != *safe_area.peek() {
                safe_area.set(current);
                // A recreated window can reset the icon colour.
                let light = matches!(*theme.peek(), crate::models::ThemeMode::Light);
                mobile_sentinel::display::set_light_system_bars(light);
            }
            crate::timer::sleep_ms(1000).await;
        }
    });

    // Read the signal reactively inside rsx! so Dioxus tracks it.
    let firing_id_val = firing_id.read().clone();

    // Read theme reactively inside rsx! so Dioxus tracks it as a dependency
    let theme_class = theme.read().css_class();
    let safe_style = safe_area.read().css_vars();

    rsx! {
           document::Link { rel: "stylesheet", href: asset!("/assets/style.css") }
           div { class: "app-root {theme_class}", style: "{safe_style}",
               if let Some(alarm_id) = firing_id_val {
    // Show alarm firing screen directly, bypassing router
                   crate::screens::AlarmFiring { key: "{alarm_id}", id: alarm_id }
               } else {
                   Router::<Route> {}
               }

    // Toast notification overlay
               if let Some(msg) = toast_signal.read().clone() {
                   crate::components::Toast {
                       msg: msg,
                       on_dismiss: move |_| { toast_signal.set(None); },
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
                bottom: i.bottom.round() as u32,
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
    use super::{remove_alarm, AlarmData};

    fn alarm(id: &str, sound: &str) -> AlarmData {
        AlarmData {
            id: id.into(),
            time_hour: 7,
            time_minute: 0,
            label: String::new(),
            days: [true; 7],
            date: None,
            skip_until_ms: None,
            fade_in_secs: 0,
            ring_limit_secs: None,
            order: 0,
            enabled: true,
            sound: sound.into(),
            sound_mode: 0,
            challenges: vec![],
            snooze_count: 3,
            snooze_interval: 5,
        }
    }

    #[test]
    fn removing_an_alarm_frees_its_custom_sound_only_when_nothing_else_uses_it() {
        let mut list = vec![
            alarm("a", "mine.mp3"),
            alarm("b", "mine.mp3"),
            alarm("c", "air"),
        ];
        assert_eq!(remove_alarm(&mut list, "a"), None, "the copy still uses it");
        assert_eq!(remove_alarm(&mut list, "b"), Some("mine.mp3".to_string()));
        assert_eq!(
            remove_alarm(&mut list, "c"),
            None,
            "bundled sounds are never deleted"
        );
        assert_eq!(remove_alarm(&mut list, "missing"), None);
        assert!(list.is_empty());
    }

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
