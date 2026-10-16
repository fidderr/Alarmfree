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

    // If the database had to be reset (or cannot persist), tell the user
    // instead of silently showing an empty alarm list.
    use_hook(|| {
        use crate::persistence::DbRecovery;
        let key = match crate::persistence::db_recovery() {
            DbRecovery::None => return,
            DbRecovery::Recovered { .. } => "db.recovered",
            DbRecovery::Volatile => "db.volatile",
        };
        toast_signal.set(Some(crate::components::ToastMessage {
            message: t(&translations, key),
            level: crate::components::ToastLevel::Error,
        }));
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
