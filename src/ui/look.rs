//! The look of the ring screen as the UI takes it: which background, in which colours, with
//! which clock. One place sets it for the ring screen and for its preview in the editor.

use slint::ComponentHandle;

use super::colour::color;
use super::{Ctx, Look};
use crate::models::{Rgb, RingBackdrop, RingClock, RingLook};

/// The accent colour of the theme in use, which a look without a colour of its own follows.
pub fn accent(ctx: &Ctx) -> Rgb {
    ctx.settings.borrow().palette(ctx.system_dark.get()).accent
}

/// Its number in the UI: the place in `RingBackdrop::ALL`.
pub fn kind(backdrop: RingBackdrop) -> i32 {
    RingBackdrop::ALL
        .iter()
        .position(|b| *b == backdrop)
        .unwrap_or(0) as i32
}

pub fn clock_style(clock: RingClock) -> i32 {
    RingClock::ALL.iter().position(|c| *c == clock).unwrap_or(0) as i32
}

/// The photo of a look, if that is what it shows and the file can be read.
pub fn load_photo(look: &RingLook) -> Option<slint::Image> {
    if look.backdrop != RingBackdrop::Photo {
        return None;
    }
    let path = crate::ring_photos::path(look.photo.as_deref()?)?;
    slint::Image::load_from_path(&path).ok()
}

/// Set the look of the ring screen.
pub fn apply(ctx: &Ctx, look: &RingLook, photo: Option<slint::Image>) {
    let Some(ui) = ctx.ui.upgrade() else {
        return;
    };
    let [c1, c2, c3] = look.colours(accent(ctx));
    let global = ui.global::<Look>();
    global.set_kind(kind(look.backdrop));
    global.set_c1(color(c1));
    global.set_c2(color(c2));
    global.set_c3(color(c3));
    global.set_has_photo(photo.is_some());
    global.set_photo(photo.unwrap_or_default());
    global.set_clock_style(clock_style(look.clock));
}
