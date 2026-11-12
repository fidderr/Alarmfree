//! The theme's colours into the `Pal` global of the UI.

use slint::{Color, ComponentHandle};

use super::{MainWindow, Pal};
use crate::models::Palette;

/// A colour as `Palette::css_vars` writes it: `#rrggbb` or `rgba(r, g, b, a)`.
fn parse_colour(text: &str) -> Option<Color> {
    let text = text.trim();
    if let Some(hex) = text.strip_prefix('#') {
        let value = u32::from_str_radix(hex, 16).ok()?;
        return (hex.len() == 6)
            .then(|| Color::from_rgb_u8((value >> 16) as u8, (value >> 8) as u8, value as u8));
    }
    let inner = text.strip_prefix("rgba(")?.strip_suffix(')')?;
    let mut parts = inner.split(',').map(str::trim);
    let r: u8 = parts.next()?.parse().ok()?;
    let g: u8 = parts.next()?.parse().ok()?;
    let b: u8 = parts.next()?.parse().ok()?;
    let a: f32 = parts.next()?.parse().ok()?;
    Some(Color::from_argb_u8(
        (a.clamp(0.0, 1.0) * 255.0).round() as u8,
        r,
        g,
        b,
    ))
}

/// The `--name: value` pairs of the palette's variables.
fn variables(palette: &Palette) -> Vec<(String, Color)> {
    palette
        .css_vars()
        .split(';')
        .filter_map(|pair| {
            let (name, value) = pair.split_once(':')?;
            Some((
                name.trim().trim_start_matches("--").to_owned(),
                parse_colour(value)?,
            ))
        })
        .collect()
}

pub fn apply(ui: &MainWindow, palette: &Palette) {
    let pal = ui.global::<Pal>();
    for (name, colour) in variables(palette) {
        match name.as_str() {
            "bg" => pal.set_bg(colour),
            "surface" => pal.set_surface(colour),
            "surface2" => pal.set_surface2(colour),
            "line" => pal.set_line(colour),
            "text" => pal.set_text(colour),
            "text2" => pal.set_text2(colour),
            "accent" => pal.set_accent(colour),
            "on-accent" => pal.set_on_accent(colour),
            "accent-soft" => pal.set_accent_soft(colour),
            "accent-glow" => pal.set_accent_glow(colour),
            "warn" => pal.set_warn(colour),
            "warn-soft" => pal.set_warn_soft(colour),
            "danger" => pal.set_danger(colour),
            "ok" => pal.set_ok(colour),
            "ok-glow" => pal.set_ok_glow(colour),
            "off" => pal.set_off(colour),
            "knob-off" => pal.set_knob_off(colour),
            "inverse" => pal.set_inverse(colour),
            "on-inverse" => pal.set_on_inverse(colour),
            "bar-soft" => pal.set_bar_soft(colour),
            _ => {}
        }
    }
    pal.set_light(palette.light);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::AppSettings;

    #[test]
    fn colours_read_as_the_palette_writes_them() {
        assert_eq!(
            parse_colour("#7cb0ff"),
            Some(Color::from_rgb_u8(0x7c, 0xb0, 0xff))
        );
        assert_eq!(
            parse_colour("rgba(124, 176, 255, 0.45)"),
            Some(Color::from_argb_u8(115, 124, 176, 255))
        );
        assert_eq!(parse_colour("nope"), None);
        assert_eq!(parse_colour("#12345"), None);
    }

    #[test]
    fn every_variable_of_a_palette_is_known() {
        let palette = AppSettings::default().palette(true);
        let names: Vec<String> = variables(&palette).into_iter().map(|(n, _)| n).collect();
        assert_eq!(names.len(), 20, "{names:?}");
    }
}
