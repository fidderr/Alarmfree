//! Choosing any colour: a wheel, how bright, and its code. What it is a colour for is up to the
//! panel around it.

use dioxus::prelude::*;

use crate::app::{t, Translations};
use crate::models::Rgb;

/// The wheel's size in CSS pixels, the same as `.color-wheel` in the style sheet.
const WHEEL_SIZE: f64 = 200.0;

/// Where on the wheel a touch is, as hue and saturation. Outside it counts as the rim.
fn wheel_point(x: f64, y: f64) -> (f32, f32) {
    let radius = WHEEL_SIZE / 2.0;
    let (dx, dy) = (x - radius, y - radius);
    let saturation = (dx.hypot(dy) / radius).min(1.0);
    let hue = dy.atan2(dx).to_degrees().rem_euclid(360.0);
    (hue as f32, saturation as f32)
}

/// Where a colour sits on the wheel, in CSS pixels.
fn wheel_position(hue: f32, saturation: f32) -> (f64, f64) {
    let radius = WHEEL_SIZE / 2.0;
    let angle = f64::from(hue).to_radians();
    (
        radius + radius * f64::from(saturation) * angle.cos(),
        radius + radius * f64::from(saturation) * angle.sin(),
    )
}

/// `hsv` and `typed` belong to the panel: it sets them from a swatch. `hsv` is kept apart from the
/// colour, because at no brightness a colour has no hue left to remember. `extra` goes next to the
/// code, and `on_release` says a finger has let go, so a good moment to save.
#[component]
pub fn ColorControls(
    hsv: Signal<(f32, f32, f32)>,
    typed: Signal<Option<String>>,
    code: String,
    on_colour: EventHandler<Rgb>,
    on_release: EventHandler<()>,
    #[props(default = VNode::empty())] extra: Element,
) -> Element {
    let translations = use_context::<Translations>();
    let mut touching = use_signal(|| false);
    let mut point_at = move |x: f64, y: f64| {
        let (hue, saturation) = wheel_point(x, y);
        let (_, _, value) = *hsv.peek();
        // On black the wheel is all black: touching it means wanting a colour.
        let value = if value < 0.05 { 1.0 } else { value };
        hsv.set((hue, saturation, value));
        typed.set(None);
        on_colour.call(Rgb::from_hsv(hue, saturation, value));
    };
    let (hue, saturation, value) = hsv();
    let (thumb_x, thumb_y) = wheel_position(hue, saturation);
    // Never all the way: on black a wheel of nothing does not look like something to touch. The
    // thumb and the code say what the colour really is.
    let dim = (1.0 - value).min(0.65);
    let shown_on_wheel = Rgb::from_hsv(hue, saturation, value).to_css();
    let brightness = (value * 100.0).round() as u32;

    rsx! {
        div {
            class: "color-wheel",
            onpointerdown: move |evt| {
                touching.set(true);
                let at = evt.element_coordinates();
                point_at(at.x, at.y);
            },
            onpointermove: move |evt| {
                if *touching.peek() {
                    let at = evt.element_coordinates();
                    point_at(at.x, at.y);
                }
            },
            onpointerup: move |_| {
                touching.set(false);
                on_release.call(());
            },
            onpointercancel: move |_| {
                touching.set(false);
                on_release.call(());
            },
            span { class: "color-wheel-dim", style: "opacity: {dim};" }
            span {
                class: "color-thumb",
                style: "left: {thumb_x}px; top: {thumb_y}px; background-color: {shown_on_wheel};",
            }
        }
        label { class: "slider-row",
            span { {t(&translations, "theme.brightness")} }
            input {
                class: "slider",
                r#type: "range",
                min: "0",
                max: "100",
                value: "{brightness}",
                oninput: move |evt| {
                    if let Ok(percent) = evt.value().parse::<f32>() {
                        let (hue, saturation, _) = *hsv.peek();
                        let value = percent / 100.0;
                        hsv.set((hue, saturation, value));
                        typed.set(None);
                        on_colour.call(Rgb::from_hsv(hue, saturation, value));
                    }
                },
                onchange: move |_| on_release.call(()),
            }
        }
        div { class: "color-code",
            input {
                class: "field color-hex",
                r#type: "text",
                maxlength: "7",
                autocapitalize: "off",
                "aria-label": t(&translations, "theme.code"),
                value: "{code}",
                oninput: move |evt| {
                    let text = evt.value();
                    if let Some(colour) = Rgb::parse(&text) {
                        hsv.set(colour.to_hsv());
                        on_colour.call(colour);
                    }
                    typed.set(Some(text));
                },
                onchange: move |_| on_release.call(()),
            }
            {extra}
        }
    }
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
}
