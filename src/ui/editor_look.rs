//! The editor's sheets for how the screen looks while this alarm rings: the background, its
//! colour and the clock. Only the draft changes here, like everywhere in the editor: it counts
//! when the alarm is saved.

use std::rc::Rc;

use slint::{ComponentHandle, SharedString};

use super::colour::{color, Wheel};
use super::editor::{edit, model, EditorState};
use super::{Ctx, Ed, EditorSheet, LookTile, SwatchCell};
use crate::i18n::t;
use crate::models::{Accent, Rgb, RingBackdrop, RingClock, RingLook};

/// The photo of the draft's look, decoded once while it stays the same.
fn photo_of(ed: &mut EditorState) -> Option<slint::Image> {
    let look = &ed.draft.ring_look;
    if look.backdrop != RingBackdrop::Photo {
        return None;
    }
    let name = look.photo.clone()?;
    if let Some((cached, image)) = &ed.photo {
        if *cached == name {
            return Some(image.clone());
        }
    }
    let image = super::look::load_photo(look)?;
    ed.photo = Some((name, image.clone()));
    Some(image)
}

/// A tile (or the live strip): `look` with `backdrop` instead of its own.
fn tile(
    ctx: &Ctx,
    look: &RingLook,
    backdrop: RingBackdrop,
    photo: Option<&slint::Image>,
) -> LookTile {
    let mut shown = look.clone();
    shown.backdrop = backdrop;
    let [c1, c2, c3] = shown.colours(super::look::accent(ctx));
    let photo = photo.filter(|_| backdrop == RingBackdrop::Photo);
    LookTile {
        name: t(&ctx.tr.borrow(), backdrop.name_key()).into(),
        kind: super::look::kind(backdrop),
        selected: look.backdrop == backdrop,
        add: backdrop == RingBackdrop::Photo && look.photo.is_none(),
        c1: color(c1),
        c2: color(c2),
        c3: color(c3),
        photo: photo.cloned().unwrap_or_default(),
        has_photo: photo.is_some(),
    }
}

pub(super) fn sheet_opened(ctx: &Rc<Ctx>, sheet: EditorSheet) {
    if sheet == EditorSheet::RingColour {
        // The wheel starts at the colour the background has now.
        let base = {
            let editor = ctx.editor.borrow();
            editor
                .as_ref()
                .map(|ed| ed.draft.ring_look.colours(super::look::accent(ctx))[0])
        };
        if let Some(base) = base {
            if let Some(ed) = ctx.editor.borrow_mut().as_mut() {
                ed.wheel = Wheel::of(base);
                ed.typed_colour = None;
            }
        }
        sync_colour(ctx, true);
    }
    sync_look(ctx);
}

/// The look sheet: the tiles, the colour row, the clock styles.
fn sync_look(ctx: &Ctx) {
    let Some(ui) = ctx.ui.upgrade() else {
        return;
    };
    let mut editor = ctx.editor.borrow_mut();
    let Some(ed) = editor.as_mut() else {
        return;
    };
    let photo = photo_of(ed);
    let look = ed.draft.ring_look.clone();
    let tiles: Vec<LookTile> = RingBackdrop::ALL
        .iter()
        .map(|backdrop| tile(ctx, &look, *backdrop, photo.as_ref()))
        .collect();
    let accent = super::look::accent(ctx);
    let tr = ctx.tr.borrow();
    let e = ui.global::<Ed>();
    e.set_tiles(model(tiles));
    e.set_photo_link(look.backdrop == RingBackdrop::Photo);
    e.set_colour_row(look.backdrop.takes_a_colour());
    e.set_colour_auto(look.colour.is_none());
    e.set_colour_dot(color(look.colours(accent)[0]));
    e.set_clock_labels(model(
        RingClock::ALL
            .iter()
            .map(|clock| SharedString::from(t(&tr, clock.name_key())))
            .collect(),
    ));
    e.set_clock_selected(
        RingClock::ALL
            .iter()
            .position(|clock| *clock == look.clock)
            .map_or(-1, |i| i as i32),
    );
    e.set_live(tile(ctx, &look, look.backdrop, photo.as_ref()));
}

/// The colour sheet: the swatches, the live strip, the wheel.
fn sync_colour(ctx: &Ctx, reset_field: bool) {
    let Some(ui) = ctx.ui.upgrade() else {
        return;
    };
    let light = ctx.settings.borrow().palette(ctx.system_dark.get()).light;
    let (look, wheel, typed) = {
        let editor = ctx.editor.borrow();
        let Some(ed) = editor.as_ref() else {
            return;
        };
        (
            ed.draft.ring_look.clone(),
            ed.wheel,
            ed.typed_colour.clone(),
        )
    };
    let swatches: Vec<SwatchCell> = Accent::PRESETS
        .iter()
        .map(|preset| {
            let colours = preset.colours();
            let colour = if light { colours.1 } else { colours.0 };
            SwatchCell {
                colour: color(colour),
                selected: look.colour == Some(colour),
            }
        })
        .collect();
    let e = ui.global::<Ed>();
    e.set_swatches(model(swatches));
    let base = look.colours(super::look::accent(ctx))[0];
    e.set_live(live_tile(ctx, &look));
    let hue = ui.global::<super::Hue>();
    hue.set_show_auto(true);
    hue.set_auto_on(look.colour.is_none());
    wheel.show(ctx, &typed.unwrap_or_else(|| base.to_css()), reset_field);
}

fn live_tile(ctx: &Ctx, look: &RingLook) -> LookTile {
    tile(ctx, look, look.backdrop, None)
}

pub fn tile_picked(ctx: &Rc<Ctx>, index: usize) {
    let Some(backdrop) = RingBackdrop::ALL.get(index).copied() else {
        return;
    };
    let no_photo_yet =
        ctx.editor.borrow().as_ref().is_some_and(|ed| {
            backdrop == RingBackdrop::Photo && ed.draft.ring_look.photo.is_none()
        });
    if no_photo_yet {
        pick_photo(ctx);
        return;
    }
    edit(ctx, |ed| ed.draft.ring_look.backdrop = backdrop);
    sync_look(ctx);
}

pub fn clock_picked(ctx: &Ctx, index: usize) {
    if let Some(clock) = RingClock::ALL.get(index).copied() {
        edit(ctx, |ed| ed.draft.ring_look.clock = clock);
        sync_look(ctx);
    }
}

/// The system's picker, off the UI thread because it waits for the user.
pub fn pick_photo(_ctx: &Rc<Ctx>) {
    std::thread::spawn(|| {
        let picked = crate::ring_photos::import();
        let _ = slint::invoke_from_event_loop(move || {
            super::with_ctx(|ctx| match picked {
                Ok(Some(name)) => {
                    edit(ctx, |ed| {
                        ed.draft.ring_look.photo = Some(name);
                        ed.draft.ring_look.backdrop = RingBackdrop::Photo;
                    });
                    sync_look(ctx);
                }
                // The user changed their mind.
                Ok(None) => {}
                Err(reason) => {
                    log::warn!("[AlarmFree] No photo imported: {reason}");
                    let message = t(&ctx.tr.borrow(), "ring.photo_failed");
                    super::toast::error(ctx, message);
                }
            });
        });
    });
}

pub fn colour_open(ctx: &Rc<Ctx>) {
    super::editor::open_sheet(ctx, EditorSheet::RingColour);
}

pub fn swatch_picked(ctx: &Ctx, index: usize) {
    let light = ctx.settings.borrow().palette(ctx.system_dark.get()).light;
    let Some(preset) = Accent::PRESETS.get(index) else {
        return;
    };
    let colours = preset.colours();
    let colour = if light { colours.1 } else { colours.0 };
    set_colour(ctx, Some(colour), true);
}

/// The draft's colour is `colour`, or the accent's again for `None`. The wheel follows.
fn set_colour(ctx: &Ctx, colour: Option<Rgb>, reset_field: bool) {
    let accent = super::look::accent(ctx);
    if let Some(ed) = ctx.editor.borrow_mut().as_mut() {
        ed.draft.ring_look.colour = colour;
        ed.typed_colour = None;
        ed.wheel = Wheel::of(ed.draft.ring_look.colours(accent)[0]);
    }
    sync_colour(ctx, reset_field);
    super::editor::sync(ctx);
}

pub fn wheel_touched(ctx: &Rc<Ctx>, x: f64, y: f64) {
    let colour = {
        let mut editor = ctx.editor.borrow_mut();
        let Some(ed) = editor.as_mut() else {
            return;
        };
        let colour = ed.wheel.touched(x, y);
        ed.draft.ring_look.colour = Some(colour);
        ed.typed_colour = None;
        colour
    };
    let _ = colour;
    sync_colour(ctx, true);
}

pub fn wheel_brightness(ctx: &Rc<Ctx>, percent: i32) {
    {
        let mut editor = ctx.editor.borrow_mut();
        let Some(ed) = editor.as_mut() else {
            return;
        };
        let colour = ed.wheel.brightness(percent);
        ed.draft.ring_look.colour = Some(colour);
        ed.typed_colour = None;
    }
    sync_colour(ctx, true);
}

/// The code typed in the field: the colour follows as soon as it is one.
pub fn wheel_code(ctx: &Rc<Ctx>, text: &str) {
    {
        let mut editor = ctx.editor.borrow_mut();
        let Some(ed) = editor.as_mut() else {
            return;
        };
        if let Some(colour) = Rgb::parse(text) {
            ed.wheel = Wheel::of(colour);
            ed.draft.ring_look.colour = Some(colour);
        }
        ed.typed_colour = Some(text.to_owned());
    }
    // Not the field: it would replace what is being typed.
    sync_colour(ctx, false);
}

pub fn wheel_auto(ctx: &Rc<Ctx>) {
    set_colour(ctx, None, true);
}

/// The ring screen as the draft would show it, without the sound.
pub fn preview(ctx: &Rc<Ctx>) {
    let Some(ui) = ctx.ui.upgrade() else {
        return;
    };
    let (alarm, photo) = {
        let mut editor = ctx.editor.borrow_mut();
        let Some(ed) = editor.as_mut() else {
            return;
        };
        let photo = photo_of(ed);
        (ed.draft.clone(), photo)
    };
    super::look::apply(ctx, &alarm.ring_look, photo);
    let tr = ctx.tr.borrow();
    let twenty_four = crate::text::uses_24_hours(&ctx.settings.borrow());
    ui.set_ring_date(super::ring::ring_date(&tr, chrono::Local::now()).into());
    ui.set_ring_clock(crate::text::clock(alarm.time_hour, alarm.time_minute, twenty_four).into());
    ui.set_ring_label(
        if alarm.label.is_empty() {
            t(&tr, "alarm.fallback_label")
        } else {
            alarm.label.clone()
        }
        .into(),
    );
    ui.set_ring_note(alarm.note.clone().into());
    ui.set_ring_snooze_text(
        t(&tr, "ring.snooze")
            .replace("{minutes}", &alarm.snooze_interval.to_string())
            .replace("{left}", &alarm.snooze_count.to_string())
            .into(),
    );
    ui.set_ring_can_snooze(alarm.snooze_count > 0);
    ui.set_ed_preview(true);
}
