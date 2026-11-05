//! Choosing the colours: a theme, an accent colour for it, or a theme of one's own. The screen
//! itself is what shows the result, so it is a screen and not a panel.

use dioxus::prelude::*;

use crate::app::{t, Translations};
use crate::back::use_back_handler;
use crate::components::{ColorControls, Icon, IconName, ScreenHeader, Sheet, SheetDone, Switch};
use crate::models::theme::{BACKGROUND_PRESETS, CARD_PRESETS, TEXT_PRESETS};
use crate::models::{Accent, AppSettings, CustomTheme, Rgb, ThemeMode};

/// What a colour is for.
#[derive(Clone, Copy, PartialEq)]
enum Role {
    Accent,
    Background,
    Card,
    Text,
}

/// The user's own colours, or the current theme's background to start from: cards and text
/// work themselves out until someone says otherwise.
fn own_colours(settings: &AppSettings, system_dark: bool) -> CustomTheme {
    settings.prefs.custom_theme.unwrap_or_else(|| CustomTheme {
        background: settings.palette(system_dark).background,
        card: None,
        text: None,
    })
}

/// Put a colour where it belongs. `None` goes back to working itself out.
fn set_colour(settings: &mut AppSettings, role: Role, colour: Option<Rgb>, system_dark: bool) {
    if role == Role::Accent {
        if let Some(colour) = colour {
            settings.prefs.accent = Accent::Custom;
            settings.prefs.custom_accent = Some(colour);
        }
        return;
    }
    let mut own = own_colours(settings, system_dark);
    match role {
        Role::Background => {
            if let Some(colour) = colour {
                own.background = colour;
            }
        }
        Role::Card => own.card = colour,
        _ => own.text = colour,
    }
    settings.prefs.custom_theme = Some(own);
}

/// What the user asked for in this role, if they did.
fn chosen_colour(settings: &AppSettings, role: Role, system_dark: bool) -> Option<Rgb> {
    let own = own_colours(settings, system_dark);
    match role {
        Role::Accent => settings
            .prefs
            .custom_accent
            .filter(|_| settings.prefs.accent == Accent::Custom),
        Role::Background => Some(own.background),
        Role::Card => own.card,
        Role::Text => own.text,
    }
}

/// What is on screen in this role.
fn shown_colour(settings: &AppSettings, role: Role, system_dark: bool) -> Rgb {
    let palette = settings.palette(system_dark);
    match role {
        Role::Accent => palette.accent,
        Role::Background => palette.background,
        Role::Card => palette.card,
        Role::Text => palette.text,
    }
}

#[component]
pub fn ThemeScreen() -> Element {
    let mut settings = use_context::<Signal<AppSettings>>();
    let translations = use_context::<Translations>();
    let nav = use_navigator();
    let mut picking: Signal<Option<Role>> = use_signal(|| None);
    use_back_handler(EventHandler::new(move |_| {
        nav.go_back();
    }));

    let system_dark = mobile_sentinel::display::system_dark();
    let now = settings.read().clone();
    let on_custom = now.theme == ThemeMode::Custom;
    let accent = now.prefs.accent;
    let shown_accent = shown_colour(&now, Role::Accent, system_dark);
    let auto_text = t(&translations, "theme.auto");
    let save = move || crate::persistence::save_settings(&settings.peek());

    let sheet_view = match picking() {
        Some(role) => rsx! {
            ColorSheet {
                key: "{role_key(role)}",
                role,
                system_dark,
                on_close: move |_| {
                    crate::persistence::save_settings(&settings.peek());
                    picking.set(None);
                },
            }
        },
        None => rsx! {},
    };

    rsx! {
        div { class: "screen",
            ScreenHeader {
                title: t(&translations, "settings.theme"),
                on_back: move |_| { nav.go_back(); },
            }

            // Fixed, so a change further down the page shows up right here.
            div { class: "theme-preview", "aria-hidden": "true",
                div { class: "theme-preview-row",
                    span { class: "alarm-time", "07:00" }
                    span { class: "alarm-info",
                        span { class: "alarm-label", {t(&translations, "config.weekdays")} }
                        span { class: "alarm-days",
                            for day in 0..7 {
                                span {
                                    class: if day < 5 { "alarm-day on" } else { "alarm-day" },
                                    {crate::text::day_letter(&translations, day)}
                                }
                            }
                        }
                    }
                    Switch { on: true }
                }
                div { class: "theme-preview-actions",
                    span { class: "theme-preview-button", {t(&translations, "action.save")} }
                    span { class: "theme-preview-link", {t(&translations, "challenge.try")} }
                }
            }

            div { class: "scroll",
                div { class: "theme-grid",
                    for mode in ThemeMode::PRESETS {
                        {
                            let selected = now.theme == mode;
                            let mut as_this = now.clone();
                            as_this.theme = mode;
                            let look = as_this.palette(system_dark);
                            let both = mode == ThemeMode::System;
                            let face = if both {
                                format!(
                                    "background: linear-gradient(135deg, {} 50%, {} 50%);",
                                    as_this.palette(true).background.to_css(),
                                    as_this.palette(false).background.to_css(),
                                )
                            } else {
                                format!("background: {};", look.background.to_css())
                            };
                            let tile_class = match (selected, both) {
                                (true, true) => "theme-tile theme-tile-both selected",
                                (true, false) => "theme-tile selected",
                                (false, true) => "theme-tile theme-tile-both",
                                (false, false) => "theme-tile",
                            };
                            rsx! {
                                button {
                                    class: "{tile_class}",
                                    "aria-pressed": if selected { "true" } else { "false" },
                                    onclick: move |_| {
                                        settings.write().theme = mode;
                                        save();
                                    },
                                    span { class: "theme-tile-face", style: "{face}",
                                        if both {
                                            span { class: "theme-tile-card",
                                                span { class: "theme-tile-dot", style: "background: {look.accent.to_css()};" }
                                            }
                                        } else {
                                            span { class: "theme-tile-card", style: "background: {look.card.to_css()};",
                                                span { class: "theme-tile-line", style: "background: {look.text.to_css()};" }
                                                span { class: "theme-tile-dot", style: "background: {look.accent.to_css()};" }
                                            }
                                        }
                                    }
                                    span { class: "theme-tile-name", {t(&translations, mode.name_key())} }
                                }
                            }
                        }
                    }
                }
                ul { class: if on_custom { "list theme-own selected" } else { "list theme-own" },
                    li {
                        button {
                            class: "row-btn",
                            "aria-pressed": if on_custom { "true" } else { "false" },
                            onclick: move |_| {
                                {
                                    let mut now = settings.write();
                                    now.prefs.custom_theme = Some(own_colours(&now, system_dark));
                                    now.theme = ThemeMode::Custom;
                                }
                                save();
                            },
                            span { class: "color-dot color-dot-rainbow", "aria-hidden": "true" }
                            span { class: "row-title", {t(&translations, "theme.custom")} }
                            span { class: "row-value" }
                        }
                    }
                }
                if on_custom {
                    ul { class: "list",
                        ColorRow {
                            title: t(&translations, "theme.background"),
                            colour: now.palette(system_dark).background,
                            on_click: move |_| picking.set(Some(Role::Background)),
                        }
                        ColorRow {
                            title: t(&translations, "theme.cards"),
                            value: if chosen_colour(&now, Role::Card, system_dark).is_none() { auto_text.clone() } else { String::new() },
                            colour: now.palette(system_dark).card,
                            on_click: move |_| picking.set(Some(Role::Card)),
                        }
                        ColorRow {
                            title: t(&translations, "theme.text"),
                            value: if chosen_colour(&now, Role::Text, system_dark).is_none() { auto_text.clone() } else { String::new() },
                            colour: now.palette(system_dark).text,
                            on_click: move |_| picking.set(Some(Role::Text)),
                        }
                    }
                }

                h2 { class: "list-title", {t(&translations, "settings.accent")} }
                div { class: "swatches swatches-page",
                    for preset in Accent::PRESETS {
                        {
                            let colours = preset.colours();
                            let colour = if now.palette(system_dark).light { colours.1 } else { colours.0 };
                            let selected = accent == preset;
                            rsx! {
                                button {
                                    class: if selected { "swatch selected" } else { "swatch" },
                                    style: "background-color: {colour.to_css()};",
                                    "aria-label": t(&translations, preset.name_key()),
                                    "aria-pressed": if selected { "true" } else { "false" },
                                    onclick: move |_| {
                                        settings.write().prefs.accent = preset;
                                        save();
                                    },
                                }
                            }
                        }
                    }
                    button {
                        class: if accent == Accent::Custom { "swatch swatch-wheel selected" } else { "swatch swatch-wheel" },
                        "aria-label": t(&translations, "accent.custom"),
                        onclick: move |_| picking.set(Some(Role::Accent)),
                        if accent == Accent::Custom {
                            span { class: "swatch-wheel-middle", style: "background-color: {shown_accent.to_css()};" }
                        }
                    }
                }
            }
            {sheet_view}
        }
    }
}

fn role_key(role: Role) -> &'static str {
    match role {
        Role::Accent => "accent",
        Role::Background => "background",
        Role::Card => "card",
        Role::Text => "text",
    }
}

/// One colour of the user's own theme.
#[component]
fn ColorRow(
    title: String,
    #[props(default)] value: String,
    colour: Rgb,
    on_click: EventHandler<()>,
) -> Element {
    rsx! {
        li {
            button { class: "row-btn", onclick: move |_| on_click.call(()),
                span { class: "row-title", "{title}" }
                span { class: "row-value", "{value}" }
                span { class: "color-dot", style: "background-color: {colour.to_css()};", "aria-hidden": "true" }
                Icon { name: IconName::ChevronRight, size: 14 }
            }
        }
    }
}

/// Picking a colour: soft ones to start from, the wheel for anything else, and a field for
/// someone who knows the code. It shows on the whole app as it changes, and is saved when
/// the finger lifts or the panel closes.
#[component]
fn ColorSheet(role: Role, system_dark: bool, on_close: EventHandler<()>) -> Element {
    let mut settings = use_context::<Signal<AppSettings>>();
    let translations = use_context::<Translations>();
    let now = settings.read().clone();
    let chosen = chosen_colour(&now, role, system_dark);
    let shown = shown_colour(&now, role, system_dark);
    let light = now.palette(system_dark).light;
    // Kept apart from the colour: at no brightness a colour has no hue left to remember.
    let mut hsv = use_signal(|| chosen.unwrap_or(shown).to_hsv());
    let mut typed: Signal<Option<String>> = use_signal(|| None);

    let presets: Vec<(Rgb, Option<Accent>)> = match role {
        Role::Accent => Accent::PRESETS
            .iter()
            .map(|preset| {
                let colours = preset.colours();
                (if light { colours.1 } else { colours.0 }, Some(*preset))
            })
            .collect(),
        Role::Background => BACKGROUND_PRESETS.iter().map(|c| (*c, None)).collect(),
        Role::Card => CARD_PRESETS.iter().map(|c| (*c, None)).collect(),
        Role::Text => TEXT_PRESETS.iter().map(|c| (*c, None)).collect(),
    };
    let title = t(
        &translations,
        match role {
            Role::Accent => "settings.accent",
            Role::Background => "theme.background",
            Role::Card => "theme.cards",
            Role::Text => "theme.text",
        },
    );
    let can_be_auto = matches!(role, Role::Card | Role::Text);
    let auto_on = can_be_auto && chosen.is_none();
    let adjusted = chosen.is_some_and(|colour| colour != shown);

    let hex_text = typed().unwrap_or_else(|| chosen.unwrap_or(shown).to_css());

    rsx! {
        Sheet {
            title,
            on_close,
            div { class: "swatches",
                for (colour, accent) in presets {
                    {
                        let selected = match accent {
                            Some(accent) => now.prefs.accent == accent,
                            None => chosen == Some(colour),
                        };
                        let label = accent.map_or_else(|| colour.to_css(), |accent| t(&translations, accent.name_key()));
                        rsx! {
                            button {
                                class: if selected { "swatch selected" } else { "swatch" },
                                style: "background-color: {colour.to_css()};",
                                "aria-label": "{label}",
                                "aria-pressed": if selected { "true" } else { "false" },
                                onclick: move |_| {
                                    hsv.set(colour.to_hsv());
                                    typed.set(None);
                                    match accent {
                                        Some(accent) => settings.write().prefs.accent = accent,
                                        None => set_colour(&mut settings.write(), role, Some(colour), system_dark),
                                    }
                                    crate::persistence::save_settings(&settings.peek());
                                },
                            }
                        }
                    }
                }
            }
            ColorControls {
                hsv,
                typed,
                code: hex_text,
                on_colour: move |colour| set_colour(&mut settings.write(), role, Some(colour), system_dark),
                on_release: move |_| {
                    crate::persistence::save_settings(&settings.peek());
                },
                extra: rsx! {
                    if can_be_auto {
                        button {
                            class: if auto_on { "chip color-auto selected" } else { "chip color-auto" },
                            "aria-pressed": if auto_on { "true" } else { "false" },
                            onclick: move |_| {
                                typed.set(None);
                                set_colour(&mut settings.write(), role, None, system_dark);
                                crate::persistence::save_settings(&settings.peek());
                                hsv.set(shown_colour(&settings.peek(), role, system_dark).to_hsv());
                            },
                            {t(&translations, "theme.auto")}
                        }
                    }
                },
            }
            if adjusted {
                p { class: "sheet-note color-adjusted", {t(&translations, "theme.adjusted")} }
            }
            SheetDone { label: t(&translations, "action.done"), on_click: move |_| on_close.call(()) }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_colour_of_ones_own_starts_from_the_theme_on_screen() {
        let mut settings = AppSettings {
            theme: ThemeMode::Sand,
            ..AppSettings::default()
        };
        let sand = settings.palette(true).background;
        set_colour(&mut settings, Role::Text, Some(Rgb::new(1, 2, 3)), true);
        let own = settings.prefs.custom_theme.unwrap();
        assert_eq!(own.background, sand);
        assert_eq!(own.text, Some(Rgb::new(1, 2, 3)));
        assert_eq!(own.card, None, "cards work themselves out");
    }

    #[test]
    fn automatic_clears_what_was_chosen() {
        let mut settings = AppSettings::default();
        set_colour(&mut settings, Role::Card, Some(Rgb::new(9, 9, 9)), true);
        assert_eq!(
            chosen_colour(&settings, Role::Card, true),
            Some(Rgb::new(9, 9, 9))
        );
        set_colour(&mut settings, Role::Card, None, true);
        assert_eq!(chosen_colour(&settings, Role::Card, true), None);
    }

    #[test]
    fn a_colour_off_the_wheel_becomes_the_accent() {
        let mut settings = AppSettings::default();
        assert_eq!(chosen_colour(&settings, Role::Accent, true), None);
        set_colour(
            &mut settings,
            Role::Accent,
            Some(Rgb::new(200, 100, 50)),
            true,
        );
        assert_eq!(settings.prefs.accent, Accent::Custom);
        assert_eq!(
            chosen_colour(&settings, Role::Accent, true),
            Some(Rgb::new(200, 100, 50))
        );
        settings.prefs.accent = Accent::Teal;
        assert_eq!(chosen_colour(&settings, Role::Accent, true), None);
    }
}
