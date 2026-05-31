use crate::app::{t, Route, Translations};
use crate::components::{Icon, IconName};
use crate::i18n;
use crate::models::{Locale, ThemeMode};
use dioxus::prelude::*;

/// Translation keys for theme labels.
const THEME_KEYS: &[&str] = &["theme.dark", "theme.light", "theme.colorblind"];

/// Display name for each Locale (in its own language).
fn locale_display(l: Locale) -> &'static str {
    match l {
        Locale::English => "English",
        Locale::Dutch => "Nederlands",
        Locale::Spanish => "Español",
        Locale::French => "Français",
        Locale::German => "Deutsch",
    }
}

/// Settings screen for app-wide preferences.
/// Uses shared context for theme and locale so changes persist across screens.
#[component]
pub fn Settings() -> Element {
    // Access global theme, locale, and translations signals from context
    let mut theme_signal = use_context::<Signal<ThemeMode>>();
    let mut locale_signal = use_context::<Signal<Locale>>();
    let mut translations = use_context::<Translations>();
    let mut theme_dropdown_open = use_signal(|| false);
    let mut lang_dropdown_open = use_signal(|| false);

    let theme_modes = [ThemeMode::Dark, ThemeMode::Light, ThemeMode::Colorblind];
    let locales = [
        Locale::English,
        Locale::Dutch,
        Locale::Spanish,
        Locale::French,
        Locale::German,
    ];

    // Translate theme labels
    let theme_labels: Vec<String> = THEME_KEYS.iter().map(|k| t(&translations, k)).collect();

    // Get current theme's translated label
    let current_theme_index = match *theme_signal.read() {
        ThemeMode::Dark => 0,
        ThemeMode::Light => 1,
        ThemeMode::Colorblind => 2,
    };
    let current_theme_label = theme_labels[current_theme_index].clone();
    let current_locale = *locale_signal.read();
    let current_language_label = locale_display(current_locale);

    let settings_title = t(&translations, "settings.title");
    let theme_label = t(&translations, "settings.theme");
    let language_label = t(&translations, "settings.language");
    let about_label = t(&translations, "settings.about");
    let tagline_text = t(&translations, "app.tagline");
    let description_text = t(&translations, "app.description");
    let version_text = t(&translations, "app.version");
    let nav_alarms = t(&translations, "nav.alarms");
    let nav_history = t(&translations, "nav.history");
    let nav_settings = t(&translations, "nav.settings");

    rsx! {
           div { class: "screen settings-screen",
    // Header
               div { class: "header",
                   h2 { "{settings_title}" }
               }

    // Theme selection (custom dropdown)
               div { class: "settings-section",
                   label { class: "section-label", "{theme_label}" }
                   div { class: "custom-dropdown",
                       button {
                           class: "dropdown-trigger",
                           onclick: move |_| {
                               let current = *theme_dropdown_open.read();
                               theme_dropdown_open.set(!current);
                               lang_dropdown_open.set(false);
                           },
                           span { class: "dropdown-value", "{current_theme_label}" }
                           span { class: "dropdown-arrow", Icon { name: IconName::ChevronDown, size: 12 } }
                       }
                       if *theme_dropdown_open.read() {
                           div { class: "dropdown-menu",
                               for (i, mode) in theme_modes.iter().enumerate() {
                                   {
                                       let is_active = current_theme_index == i;
                                       let mode_val = *mode;
                                       let label = theme_labels[i].clone();
                                       rsx! {
                                           button {
                                               class: if is_active { "dropdown-item active" } else { "dropdown-item" },
                                               onclick: move |_| {
                                                   theme_signal.set(mode_val);
                                                   theme_dropdown_open.set(false);
                                                   let mut settings = crate::persistence::load_settings();
                                                   settings.theme = mode_val;
                                                   crate::persistence::save_settings(&settings);
                                               },
                                               "{label}"
                                           }
                                       }
                                   }
                               }
                           }
                       }
                   }
               }

    // Language selection (custom dropdown)
               div { class: "settings-section",
                   label { class: "section-label", "{language_label}" }
                   div { class: "custom-dropdown",
                       button {
                           class: "dropdown-trigger",
                           onclick: move |_| {
                               let current = *lang_dropdown_open.read();
                               lang_dropdown_open.set(!current);
                               theme_dropdown_open.set(false);
                           },
                           span { class: "dropdown-value", "{current_language_label}" }
                           span { class: "dropdown-arrow", Icon { name: IconName::ChevronDown, size: 12 } }
                       }
                       if *lang_dropdown_open.read() {
                           div { class: "dropdown-menu",
                               for locale_val in locales.iter() {
                                   {
                                       let lv = *locale_val;
                                       let is_active = current_locale == lv;
                                       let display = locale_display(lv);
                                       rsx! {
                                           button {
                                               class: if is_active { "dropdown-item active" } else { "dropdown-item" },
                                               onclick: move |_| {
                                                   locale_signal.set(lv);
                                                   translations.set(i18n::load_translations(lv));
                                                   lang_dropdown_open.set(false);
                                                   let mut settings = crate::persistence::load_settings();
                                                   settings.locale = lv;
                                                   crate::persistence::save_settings(&settings);
                                               },
                                               "{display}"
                                           }
                                       }
                                   }
                               }
                           }
                       }
                   }
               }

    // About section
               div { class: "settings-section",
                   label { class: "section-label", "{about_label}" }
                   div { class: "about-info",
                       p { class: "about-name", "{version_text}" }
                       p { class: "about-desc", "{tagline_text}" }
                       p { class: "about-desc", "{description_text}" }
                   }
               }

    // Notification permission deep-link.
    // Opens android.settings.APP_NOTIFICATION_SETTINGS so the user
    // can re-grant POST_NOTIFICATIONS after initial denial.
    // Hidden on non-Android — the link target is Android-only.
               {
    #[cfg(target_os = "android")]
                   {
                       let notifications_label = t(&translations, "settings.notifications");
                       let open_notification_text = t(&translations, "settings.open_notification_settings");
                       let display_over_apps_label = t(&translations, "settings.display_over_apps");
                       let overlay_desc_text = t(&translations, "settings.overlay_description");
                       let grant_overlay_text = t(&translations, "settings.grant_overlay");
                       rsx! {
                           div { class: "settings-section",
                               label { class: "section-label", "{notifications_label}" }
                               button {
                                   class: "dropdown-trigger",
                                   onclick: move |_| {
                                       open_app_notification_settings();
                                   },
                                   span { class: "dropdown-value", "{open_notification_text}" }
                                   span { class: "dropdown-arrow", Icon { name: IconName::ChevronRight, size: 12 } }
                               }
                           }

    // SYSTEM_ALERT_WINDOW deep-link — REQUIRED for
    // Alarmy-style force-to-front when the screen is on
    // and unlocked. Without it, alarms only fire as a
    // heads-up notification in that state. See
    // `platform.rs::init_sentinel` for the init-time
    // prompt; this row lets the user re-check / re-grant
    // later if they dismissed the prompt.
                           div { class: "settings-section",
                               label { class: "section-label", "{display_over_apps_label}" }
                               p { class: "about-desc", "{overlay_desc_text}" }
                               button {
                                   class: "dropdown-trigger",
                                   onclick: move |_| {
                                       request_system_alert_window();
                                   },
                                   span { class: "dropdown-value", "{grant_overlay_text}" }
                                   span { class: "dropdown-arrow", Icon { name: IconName::ChevronRight, size: 12 } }
                               }
                           }
                       }
                   }
    #[cfg(not(target_os = "android"))]
                   rsx! {}
               }

    // Bottom navigation
               nav { class: "bottom-nav",
                   Link { to: Route::Home {}, class: "nav-item",
                       Icon { name: IconName::Clock }
                       " {nav_alarms}"
                   }
                   Link { to: Route::History {}, class: "nav-item",
                       Icon { name: IconName::ChartBar }
                       " {nav_history}"
                   }
                   Link { to: Route::Settings {}, class: "nav-item active",
                       Icon { name: IconName::Gear }
                       " {nav_settings}"
                   }
               }
           }
       }
}

/// Open the app's notification settings page so the user can re-grant
/// POST_NOTIFICATIONS after denying the initial runtime prompt.
#[cfg(target_os = "android")]
fn open_app_notification_settings() {
    mobile_sentinel::permissions::open_app_settings();
}

/// Launch the `ACTION_MANAGE_OVERLAY_PERMISSION` Settings page so the user
/// can grant `SYSTEM_ALERT_WINDOW` — the permission that lets the alarm
/// receiver force the firing screen to foreground when the phone is on
/// and unlocked. Without this permission, Android 14's Background
/// Activity Launch (BAL) restrictions collapse our `startActivity` call
/// to a heads-up notification. Delegates to `mobile_sentinel::overlay::request()`.
#[cfg(target_os = "android")]
fn request_system_alert_window() {
    mobile_sentinel::overlay::request();
    log::info!("[AlarmFree] Overlay-permission Settings launched");
}
