//! Choosing any colour: a wheel, how bright, and its code. What it is a colour for is up to
//! the panel around it. The wheel itself is drawn by `ui/colour.slint`; this keeps what it
//! shows and turns touches into colours.

use std::rc::Rc;

use slint::{Color, ComponentHandle};

use super::{Ctx, Hue};
use crate::models::Rgb;

/// The wheel's size in logical pixels, the same as in `ui/colour.slint`.
const WHEEL_SIZE: f64 = 200.0;

/// Where on the wheel a touch is, as hue and saturation. Outside it counts as the rim.
pub fn wheel_point(x: f64, y: f64) -> (f32, f32) {
    let radius = WHEEL_SIZE / 2.0;
    let (dx, dy) = (x - radius, y - radius);
    let saturation = (dx.hypot(dy) / radius).min(1.0);
    let hue = dy.atan2(dx).to_degrees().rem_euclid(360.0);
    (hue as f32, saturation as f32)
}

/// Where a colour sits on the wheel, in logical pixels.
pub fn wheel_position(hue: f32, saturation: f32) -> (f64, f64) {
    let radius = WHEEL_SIZE / 2.0;
    let angle = f64::from(hue).to_radians();
    (
        radius + radius * f64::from(saturation) * angle.cos(),
        radius + radius * f64::from(saturation) * angle.sin(),
    )
}

pub fn color(colour: Rgb) -> Color {
    Color::from_rgb_u8(colour.r, colour.g, colour.b)
}

/// What the wheel and its fields work on. `hsv` is kept apart from the colour, because at
/// no brightness a colour has no hue left to remember.
#[derive(Clone, Copy)]
pub struct Wheel {
    pub hsv: (f32, f32, f32),
}

impl Wheel {
    pub fn of(colour: Rgb) -> Self {
        Self {
            hsv: colour.to_hsv(),
        }
    }

    pub fn colour(self) -> Rgb {
        Rgb::from_hsv(self.hsv.0, self.hsv.1, self.hsv.2)
    }

    /// A touch on the wheel. On black the wheel is all black: touching it means wanting a
    /// colour.
    pub fn touched(&mut self, x: f64, y: f64) -> Rgb {
        let (hue, saturation) = wheel_point(x, y);
        let value = if self.hsv.2 < 0.05 { 1.0 } else { self.hsv.2 };
        self.hsv = (hue, saturation, value);
        self.colour()
    }

    pub fn brightness(&mut self, percent: i32) -> Rgb {
        self.hsv.2 = percent.clamp(0, 100) as f32 / 100.0;
        self.colour()
    }

    /// Show the wheel: thumb, brightness and code. `typed` is what is in the field while
    /// editing, else the code of the colour.
    pub fn show(self, ctx: &Ctx, code: &str, reset_field: bool) {
        let Some(ui) = ctx.ui.upgrade() else {
            return;
        };
        let (hue, saturation, value) = self.hsv;
        let (x, y) = wheel_position(hue, saturation);
        let hue_global = ui.global::<Hue>();
        hue_global.set_thumb_x(x as f32);
        hue_global.set_thumb_y(y as f32);
        hue_global.set_shown(color(self.colour()));
        // Never all the way: on black a wheel of nothing does not look like something to
        // touch. The thumb and the code say what the colour really is.
        hue_global.set_dim((1.0 - value).min(0.65));
        hue_global.set_brightness((value * 100.0).round() as i32);
        hue_global.set_code(code.into());
        if reset_field {
            hue_global.set_code_rev(hue_global.get_code_rev() + 1);
        }
    }
}

/// The wheel's callbacks are those of whichever panel has it open.
pub fn connect(ctx: &Rc<Ctx>, ui: &super::MainWindow) {
    let hue = ui.global::<Hue>();
    let weak = Rc::downgrade(ctx);
    hue.on_touch(move |x, y| {
        if let Some(ctx) = weak.upgrade() {
            super::editor_look::wheel_touched(&ctx, f64::from(x), f64::from(y));
        }
    });
    let weak = Rc::downgrade(ctx);
    hue.on_brightness_changed(move |percent| {
        if let Some(ctx) = weak.upgrade() {
            super::editor_look::wheel_brightness(&ctx, percent);
        }
    });
    let weak = Rc::downgrade(ctx);
    hue.on_code_edited(move |text| {
        if let Some(ctx) = weak.upgrade() {
            super::editor_look::wheel_code(&ctx, text.as_str());
        }
    });
    let weak = Rc::downgrade(ctx);
    hue.on_auto_picked(move || {
        if let Some(ctx) = weak.upgrade() {
            super::editor_look::wheel_auto(&ctx);
        }
    });
    hue.on_released(|| {});
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_touch_on_the_wheel_gives_the_colour_it_shows() {
        let red = wheel_point(WHEEL_SIZE, WHEEL_SIZE / 2.0);
        assert!(
            (red.0 - 0.0).abs() < 0.5 && (red.1 - 1.0).abs() < 0.01,
            "{red:?}"
        );
        // The gradient runs clockwise from red, on the right.
        let down = wheel_point(WHEEL_SIZE / 2.0, WHEEL_SIZE);
        assert!((down.0 - 90.0).abs() < 0.5, "{down:?}");
        let middle = wheel_point(WHEEL_SIZE / 2.0, WHEEL_SIZE / 2.0);
        assert!(middle.1 < 0.01, "white in the middle: {middle:?}");
        let outside = wheel_point(WHEEL_SIZE + 50.0, WHEEL_SIZE / 2.0);
        assert!(
            (outside.1 - 1.0).abs() < 0.01,
            "outside is the rim: {outside:?}"
        );
    }

    #[test]
    fn a_colour_comes_back_to_where_it_was_touched() {
        for (x, y) in [(10.0, 100.0), (160.0, 50.0), (100.0, 100.0), (140.0, 170.0)] {
            let (hue, saturation) = wheel_point(x, y);
            let (again_x, again_y) = wheel_position(hue, saturation);
            assert!((again_x - x).abs() < 0.5 && (again_y - y).abs() < 0.5);
        }
    }

    #[test]
    fn touching_a_black_wheel_brings_the_colour_back() {
        let mut wheel = Wheel::of(Rgb::new(0, 0, 0));
        let colour = wheel.touched(WHEEL_SIZE, WHEEL_SIZE / 2.0);
        assert_eq!(colour, Rgb::new(255, 0, 0));
    }
}
