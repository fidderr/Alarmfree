//! Choosing the colours: a theme, an accent colour for it, or a theme of one's own. The screen
//! itself is what shows the result, so it is a screen and not a panel.

use std::rc::Rc;

use slint::ComponentHandle;

use super::colour::{color, Wheel};
use super::editor::model;
use super::{Ctx, DayCell, Hue, SwatchCell, ThemeTile, Thm, SCREEN_THEME};
use crate::i18n::t;
use crate::models::theme::{BACKGROUND_PRESETS, CARD_PRESETS, TEXT_PRESETS};
use crate::models::{Accent, AppSettings, CustomTheme, Rgb, ThemeMode};

/// What a colour is for.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Role {
    Accent,
    Background,
    Card,
    Text,
}

impl Role {
    fn from_index(index: i32) -> Option<Self> {
        match index {
            0 => Some(Role::Accent),
            1 => Some(Role::Background),
            2 => Some(Role::Card),
            3 => Some(Role::Text),
            _ => None,
        }
    }
}

/// The colour sheet that is open, if one is.
pub struct State {
    picking: Option<Role>,
    wheel: Wheel,
    typed: Option<String>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            picking: None,
            wheel: Wheel::of(Rgb::new(255, 255, 255)),
            typed: None,
        }
    }
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

pub fn open(ctx: &Ctx) {
    let Some(ui) = ctx.ui.upgrade() else {
        return;
    };
    *ctx.theme.borrow_mut() = State::default();
    ui.set_panel_open(false);
    ui.set_screen(SCREEN_THEME);
    sync(ctx);
}

/// Back: the colour sheet first, else the settings.
pub fn leave(ctx: &Rc<Ctx>) {
    if ctx.theme.borrow().picking.is_some() {
        close_sheet(ctx);
        return;
    }
    super::settings::enter(ctx);
}

/// Whether the colour sheet has the wheel at the moment.
pub fn has_wheel(ctx: &Ctx) -> bool {
    ctx.theme.borrow().picking.is_some()
}

fn save(ctx: &Ctx) {
    crate::persistence::save_settings(&ctx.settings.borrow());
}

/// A setting changed: show it on the whole app, and on this screen.
fn changed(ctx: &Ctx) {
    super::apply_theme(ctx);
    sync(ctx);
}

/// Draw the screen from the settings.
pub fn sync(ctx: &Ctx) {
    let Some(ui) = ctx.ui.upgrade() else {
        return;
    };
    let dark = ctx.system_dark.get();
    let settings = ctx.settings.borrow().clone();
    let tr = ctx.tr.borrow();
    let palette = settings.palette(dark);
    let thm = ui.global::<Thm>();

    thm.set_tiles(model(
        ThemeMode::PRESETS
            .iter()
            .map(|mode| {
                let mut as_this = settings.clone();
                as_this.theme = *mode;
                let look = as_this.palette(dark);
                let both = *mode == ThemeMode::System;
                ThemeTile {
                    name: t(&tr, mode.name_key()).into(),
                    bg: color(if both {
                        as_this.palette(true).background
                    } else {
                        look.background
                    }),
                    bg2: color(as_this.palette(false).background),
                    card: color(look.card),
                    ink: color(look.text),
                    accent: color(look.accent),
                    both,
                    selected: settings.theme == *mode,
                }
            })
            .collect(),
    ));
    thm.set_own_selected(settings.theme == ThemeMode::Custom);
    thm.set_own_bg(color(palette.background));
    thm.set_own_card(color(palette.card));
    thm.set_own_ink(color(palette.text));
    thm.set_card_auto(chosen_colour(&settings, Role::Card, dark).is_none());
    thm.set_ink_auto(chosen_colour(&settings, Role::Text, dark).is_none());
    thm.set_accents(model(
        Accent::PRESETS
            .iter()
            .map(|preset| SwatchCell {
                colour: color(shade(*preset, palette.light)),
                selected: settings.prefs.accent == *preset,
            })
            .collect(),
    ));
    thm.set_custom_accent(settings.prefs.accent == Accent::Custom);
    thm.set_custom_accent_colour(color(palette.accent));
    thm.set_days(model(
        (0..7)
            .map(|day| DayCell {
                letter: crate::text::day_letter(&tr, day).into(),
                on: day < 5,
            })
            .collect(),
    ));
    thm.set_weekdays(t(&tr, "config.weekdays").into());
    drop(tr);
    sync_sheet(ctx, false);
}

fn shade(accent: Accent, light: bool) -> Rgb {
    let colours = accent.colours();
    if light {
        colours.1
    } else {
        colours.0
    }
}

/// The colour sheet: the swatches to start from, the wheel and the note.
fn sync_sheet(ctx: &Ctx, reset_field: bool) {
    let Some(ui) = ctx.ui.upgrade() else {
        return;
    };
    let thm = ui.global::<Thm>();
    let (role, wheel, typed) = {
        let state = ctx.theme.borrow();
        (state.picking, state.wheel, state.typed.clone())
    };
    let Some(role) = role else {
        thm.set_sheet_open(false);
        return;
    };
    let dark = ctx.system_dark.get();
    let settings = ctx.settings.borrow();
    let tr = ctx.tr.borrow();
    let light = settings.palette(dark).light;
    let chosen = chosen_colour(&settings, role, dark);
    let shown = shown_colour(&settings, role, dark);
    let swatches: Vec<SwatchCell> = match role {
        Role::Accent => Accent::PRESETS
            .iter()
            .map(|preset| SwatchCell {
                colour: color(shade(*preset, light)),
                selected: settings.prefs.accent == *preset,
            })
            .collect(),
        Role::Background => presets(&BACKGROUND_PRESETS, chosen),
        Role::Card => presets(&CARD_PRESETS, chosen),
        Role::Text => presets(&TEXT_PRESETS, chosen),
    };
    thm.set_sheet_title(
        t(
            &tr,
            match role {
                Role::Accent => "settings.accent",
                Role::Background => "theme.background",
                Role::Card => "theme.cards",
                Role::Text => "theme.text",
            },
        )
        .into(),
    );
    thm.set_sheet_swatches(model(swatches));
    thm.set_adjusted(chosen.is_some_and(|colour| colour != shown));
    thm.set_sheet_open(true);
    let hue = ui.global::<Hue>();
    let can_be_auto = matches!(role, Role::Card | Role::Text);
    hue.set_show_auto(can_be_auto);
    hue.set_auto_on(can_be_auto && chosen.is_none());
    let code = typed.unwrap_or_else(|| chosen.unwrap_or(shown).to_css());
    wheel.show(ctx, &code, reset_field);
}

fn presets(list: &[Rgb], chosen: Option<Rgb>) -> Vec<SwatchCell> {
    list.iter()
        .map(|colour| SwatchCell {
            colour: color(*colour),
            selected: chosen == Some(*colour),
        })
        .collect()
}

pub fn tile_picked(ctx: &Ctx, index: usize) {
    let Some(mode) = ThemeMode::PRESETS.get(index).copied() else {
        return;
    };
    ctx.settings.borrow_mut().theme = mode;
    save(ctx);
    changed(ctx);
}

pub fn own_picked(ctx: &Ctx) {
    {
        let dark = ctx.system_dark.get();
        let mut settings = ctx.settings.borrow_mut();
        let own = own_colours(&settings, dark);
        settings.prefs.custom_theme = Some(own);
        settings.theme = ThemeMode::Custom;
    }
    save(ctx);
    changed(ctx);
}

pub fn accent_picked(ctx: &Ctx, index: usize) {
    let Some(preset) = Accent::PRESETS.get(index).copied() else {
        return;
    };
    ctx.settings.borrow_mut().prefs.accent = preset;
    save(ctx);
    changed(ctx);
}

pub fn role_open(ctx: &Ctx, index: i32) {
    let Some(role) = Role::from_index(index) else {
        return;
    };
    {
        let dark = ctx.system_dark.get();
        let settings = ctx.settings.borrow();
        let colour =
            chosen_colour(&settings, role, dark).unwrap_or(shown_colour(&settings, role, dark));
        *ctx.theme.borrow_mut() = State {
            picking: Some(role),
            wheel: Wheel::of(colour),
            typed: None,
        };
    }
    sync_sheet(ctx, true);
}

pub fn close_sheet(ctx: &Ctx) {
    ctx.theme.borrow_mut().picking = None;
    save(ctx);
    sync_sheet(ctx, false);
}

/// A swatch of the colour sheet: the colour (or accent) it shows.
pub fn sheet_swatch_picked(ctx: &Ctx, index: usize) {
    let Some(role) = ctx.theme.borrow().picking else {
        return;
    };
    let dark = ctx.system_dark.get();
    let colour = {
        let settings = ctx.settings.borrow();
        let light = settings.palette(dark).light;
        match role {
            Role::Accent => Accent::PRESETS
                .get(index)
                .map(|preset| (shade(*preset, light), Some(*preset))),
            Role::Background => BACKGROUND_PRESETS.get(index).map(|c| (*c, None)),
            Role::Card => CARD_PRESETS.get(index).map(|c| (*c, None)),
            Role::Text => TEXT_PRESETS.get(index).map(|c| (*c, None)),
        }
    };
    let Some((colour, accent)) = colour else {
        return;
    };
    {
        let mut state = ctx.theme.borrow_mut();
        state.wheel = Wheel::of(colour);
        state.typed = None;
    }
    match accent {
        Some(accent) => ctx.settings.borrow_mut().prefs.accent = accent,
        None => set_colour(&mut ctx.settings.borrow_mut(), role, Some(colour), dark),
    }
    save(ctx);
    super::apply_theme(ctx);
    sync(ctx);
    sync_sheet(ctx, true);
}

/// A colour from the wheel, the brightness or the code: shown at once, saved when the
/// finger lifts or the sheet closes.
fn apply_colour(ctx: &Ctx, colour: Rgb, reset_field: bool) {
    let Some(role) = ctx.theme.borrow().picking else {
        return;
    };
    set_colour(
        &mut ctx.settings.borrow_mut(),
        role,
        Some(colour),
        ctx.system_dark.get(),
    );
    super::apply_theme(ctx);
    sync(ctx);
    sync_sheet(ctx, reset_field);
}

pub fn wheel_touched(ctx: &Ctx, x: f64, y: f64) {
    let colour = {
        let mut state = ctx.theme.borrow_mut();
        state.typed = None;
        state.wheel.touched(x, y)
    };
    apply_colour(ctx, colour, true);
}

pub fn wheel_brightness(ctx: &Ctx, percent: i32) {
    let colour = {
        let mut state = ctx.theme.borrow_mut();
        state.typed = None;
        state.wheel.brightness(percent)
    };
    apply_colour(ctx, colour, true);
}

pub fn wheel_code(ctx: &Ctx, text: &str) {
    let parsed = Rgb::parse(text);
    {
        let mut state = ctx.theme.borrow_mut();
        if let Some(colour) = parsed {
            state.wheel = Wheel::of(colour);
        }
        state.typed = Some(text.to_owned());
    }
    match parsed {
        Some(colour) => {
            apply_colour(ctx, colour, false);
            save(ctx);
        }
        None => sync_sheet(ctx, false),
    }
}

/// Cards and text back to working themselves out.
pub fn wheel_auto(ctx: &Ctx) {
    let Some(role) = ctx.theme.borrow().picking else {
        return;
    };
    let dark = ctx.system_dark.get();
    set_colour(&mut ctx.settings.borrow_mut(), role, None, dark);
    save(ctx);
    super::apply_theme(ctx);
    let shown = shown_colour(&ctx.settings.borrow(), role, dark);
    {
        let mut state = ctx.theme.borrow_mut();
        state.wheel = Wheel::of(shown);
        state.typed = None;
    }
    sync(ctx);
    sync_sheet(ctx, true);
}

pub fn released(ctx: &Ctx) {
    if ctx.theme.borrow().picking.is_some() {
        save(ctx);
    }
}

pub fn connect(ctx: &Rc<Ctx>, ui: &super::MainWindow) {
    let thm = ui.global::<Thm>();
    macro_rules! on {
        ($setter:ident, |$ctx:ident $(, $arg:ident)*| $body:expr) => {{
            let weak = Rc::downgrade(ctx);
            thm.$setter(move |$($arg),*| {
                if let Some($ctx) = weak.upgrade() {
                    $body
                }
            });
        }};
    }
    on!(on_back, |ctx| leave(&ctx));
    on!(on_tile_picked, |ctx, i| tile_picked(&ctx, i as usize));
    on!(on_own_picked, |ctx| own_picked(&ctx));
    on!(on_accent_picked, |ctx, i| accent_picked(&ctx, i as usize));
    on!(on_role_open, |ctx, i| role_open(&ctx, i));
    on!(on_sheet_swatch_picked, |ctx, i| sheet_swatch_picked(
        &ctx, i as usize
    ));
    on!(on_sheet_close, |ctx| close_sheet(&ctx));
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

    #[test]
    fn the_roles_of_the_screen_are_numbered_as_the_slint_side_does() {
        assert_eq!(Role::from_index(0), Some(Role::Accent));
        assert_eq!(Role::from_index(3), Some(Role::Text));
        assert_eq!(Role::from_index(4), None);
    }
}
