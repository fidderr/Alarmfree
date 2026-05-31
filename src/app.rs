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
    pub enabled: bool,
    pub sound: String,
    pub sound_mode: usize,                 // 0=Sound+Vib, 1=Sound, 2=Vib
    pub challenges: Vec<(String, String)>, // (type, difficulty)
    pub snooze_count: u8,
    pub snooze_interval: u8,
}

/// Root App component with global state context providers.
#[component]
pub fn App() -> Element {
    // Load persisted settings (theme + locale).
    let settings = crate::persistence::load_settings();

    // Extract bundled sounds from APK assets to internal storage (idempotent)
    crate::custom_sounds::extract_bundled_sounds();

    // Global state signals provided via context.
    let theme = use_signal(|| settings.theme);
    let locale = use_signal(|| settings.locale);
    let alarms = use_signal(crate::persistence::load_alarms);
    let translations: Translations = use_signal(|| i18n::load_translations(settings.locale));

    // Initialize platform: logger, FiringSink, AlarmKit, alarm rearm.
    use_hook(crate::platform::init_sentinel);

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

    // Firing-state signal — primed from disk on mount, then driven by the
    // single 500 ms ContextStore poller in `firing_state::init_notifier`.
    // The poller pushes a new value whenever the firing instance id changes
    // (cross-process write by the Recipe state machine), so Dioxus re-renders
    // without any UI-side poll.
    let mut firing_id: Signal<Option<String>> = use_signal(crate::firing_state::peek_firing_alarm);

    use_future(move || {
        let mut rx = crate::firing_state::init_notifier();
        async move {
            use futures_util::StreamExt;
            while let Some(next) = rx.next().await {
                firing_id.set(next);
            }
        }
    });

    // Read the signal reactively inside rsx! so Dioxus tracks it.
    let firing_id_val = firing_id.read().clone();

    // Read theme reactively inside rsx! so Dioxus tracks it as a dependency
    let theme_class = theme.read().css_class();

    rsx! {
           document::Link { rel: "stylesheet", href: asset!("/assets/style.css") }
           div { class: "app-root {theme_class}",
               if let Some(alarm_id) = firing_id_val {
    // Show alarm firing screen directly, bypassing router
                   crate::screens::AlarmFiring { id: alarm_id }
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
