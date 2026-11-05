//! The colours of the app. A theme is four colours, and everything else on screen follows
//! from those here: the oldest WebView the app runs on has no `color-mix`.

use serde::{Deserialize, Serialize};

/// A colour, saved as "#rrggbb".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

/// Text and what stands out never come closer to what is behind them than this. An alarm has
/// to be readable at seven in the morning, whatever colours were picked.
pub const MIN_CONTRAST: f32 = 3.0;

const WHITE: Rgb = Rgb::new(255, 255, 255);
const BLACK: Rgb = Rgb::new(0, 0, 0);

impl Rgb {
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    const fn hex(value: u32) -> Self {
        Self::new((value >> 16) as u8, (value >> 8) as u8, value as u8)
    }

    /// "#rrggbb", "rrggbb" or "#rgb", as someone might type it.
    pub fn parse(text: &str) -> Option<Self> {
        let text = text.trim().trim_start_matches('#');
        if !text.is_ascii() {
            return None;
        }
        let part = |range: std::ops::Range<usize>| u8::from_str_radix(&text[range], 16).ok();
        match text.len() {
            3 => {
                let double = |digit: u8| digit * 17;
                Some(Self::new(
                    double(part(0..1)?),
                    double(part(1..2)?),
                    double(part(2..3)?),
                ))
            }
            6 => Some(Self::new(part(0..2)?, part(2..4)?, part(4..6)?)),
            _ => None,
        }
    }

    pub fn to_css(self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
    }

    /// This colour with `amount` (0 to 1) of another one in it.
    pub fn mix(self, other: Rgb, amount: f32) -> Rgb {
        let one = |a: u8, b: u8| {
            (f32::from(a) + (f32::from(b) - f32::from(a)) * amount)
                .round()
                .clamp(0.0, 255.0) as u8
        };
        Rgb::new(
            one(self.r, other.r),
            one(self.g, other.g),
            one(self.b, other.b),
        )
    }

    fn luminance(self) -> f32 {
        let linear = |channel: u8| {
            let value = f32::from(channel) / 255.0;
            if value <= 0.039_28 {
                value / 12.92
            } else {
                ((value + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * linear(self.r) + 0.7152 * linear(self.g) + 0.0722 * linear(self.b)
    }

    /// How far apart two colours are for the eye, from 1 to 21.
    pub fn contrast(self, other: Rgb) -> f32 {
        let (a, b) = (self.luminance(), other.luminance());
        (a.max(b) + 0.05) / (a.min(b) + 0.05)
    }

    /// Whether dark text reads better on this than light text.
    pub fn is_light(self) -> bool {
        self.contrast(BLACK) > self.contrast(WHITE)
    }

    /// As little changed as it takes to read against both backgrounds. Colours that already do
    /// stay exactly as they are.
    pub fn readable_on(self, one: Rgb, other: Rgb) -> Rgb {
        let score = |colour: Rgb| colour.contrast(one).min(colour.contrast(other));
        if score(self) >= MIN_CONTRAST {
            return self;
        }
        let mut best = self;
        for step in 1..=20 {
            let amount = step as f32 / 20.0;
            for toward in [WHITE, BLACK] {
                let candidate = self.mix(toward, amount);
                if score(candidate) >= MIN_CONTRAST {
                    return candidate;
                }
                if score(candidate) > score(best) {
                    best = candidate;
                }
            }
        }
        best
    }

    /// Hue in degrees, saturation and brightness from 0 to 1.
    pub fn to_hsv(self) -> (f32, f32, f32) {
        let (r, g, b) = (
            f32::from(self.r) / 255.0,
            f32::from(self.g) / 255.0,
            f32::from(self.b) / 255.0,
        );
        let max = r.max(g).max(b);
        let spread = max - r.min(g).min(b);
        let hue = if spread == 0.0 {
            0.0
        } else if max == r {
            60.0 * ((g - b) / spread).rem_euclid(6.0)
        } else if max == g {
            60.0 * ((b - r) / spread + 2.0)
        } else {
            60.0 * ((r - g) / spread + 4.0)
        };
        let saturation = if max == 0.0 { 0.0 } else { spread / max };
        (hue, saturation, max)
    }

    pub fn from_hsv(hue: f32, saturation: f32, value: f32) -> Rgb {
        let chroma = value * saturation;
        let sector = hue.rem_euclid(360.0) / 60.0;
        let second = chroma * (1.0 - (sector.rem_euclid(2.0) - 1.0).abs());
        let (r, g, b) = match sector as u32 {
            0 => (chroma, second, 0.0),
            1 => (second, chroma, 0.0),
            2 => (0.0, chroma, second),
            3 => (0.0, second, chroma),
            4 => (second, 0.0, chroma),
            _ => (chroma, 0.0, second),
        };
        let channel = |part: f32| ((part + value - chroma) * 255.0).round().clamp(0.0, 255.0) as u8;
        Rgb::new(channel(r), channel(g), channel(b))
    }
}

impl TryFrom<String> for Rgb {
    type Error = String;

    fn try_from(text: String) -> Result<Self, String> {
        Rgb::parse(&text).ok_or_else(|| format!("not a colour: {text}"))
    }
}

impl From<Rgb> for String {
    fn from(colour: Rgb) -> String {
        colour.to_css()
    }
}

/// The colours someone chose for their own theme. Cards and text work themselves out unless
/// they say otherwise.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustomTheme {
    pub background: Rgb,
    #[serde(default)]
    pub card: Option<Rgb>,
    #[serde(default)]
    pub text: Option<Rgb>,
}

/// The base colours of a theme that comes with the app.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BaseColours {
    pub background: Rgb,
    pub card: Rgb,
    pub text: Rgb,
}

impl BaseColours {
    const fn new(background: u32, card: u32, text: u32) -> Self {
        Self {
            background: Rgb::hex(background),
            card: Rgb::hex(card),
            text: Rgb::hex(text),
        }
    }
}

pub const DARK: BaseColours = BaseColours::new(0x101218, 0x1b1e27, 0xf2f3f7);
pub const LIGHT: BaseColours = BaseColours::new(0xf5f6f9, 0xffffff, 0x14161c);
pub const BLACK_THEME: BaseColours = BaseColours::new(0x000000, 0x101116, 0xf2f3f7);
pub const MIDNIGHT: BaseColours = BaseColours::new(0x0b1220, 0x162033, 0xeef2fa);
pub const FOREST: BaseColours = BaseColours::new(0x0e1714, 0x18241f, 0xeef4f0);
pub const PLUM: BaseColours = BaseColours::new(0x17101c, 0x241a2c, 0xf4eff7);
pub const SAND: BaseColours = BaseColours::new(0xf3ede2, 0xfbf7ef, 0x2a2419);
pub const MIST: BaseColours = BaseColours::new(0xe9eef2, 0xf7f9fb, 0x1c252c);

/// Soft colours to start from, so nobody has to find a good one on the wheel.
pub const BACKGROUND_PRESETS: [Rgb; 10] = [
    Rgb::hex(0x101218),
    Rgb::hex(0x000000),
    Rgb::hex(0x0b1220),
    Rgb::hex(0x0e1714),
    Rgb::hex(0x17101c),
    Rgb::hex(0x1a1411),
    Rgb::hex(0xf3ede2),
    Rgb::hex(0xe9eef2),
    Rgb::hex(0xf7eaec),
    Rgb::hex(0xffffff),
];

pub const CARD_PRESETS: [Rgb; 10] = [
    Rgb::hex(0x1b1e27),
    Rgb::hex(0x101116),
    Rgb::hex(0x162033),
    Rgb::hex(0x18241f),
    Rgb::hex(0x241a2c),
    Rgb::hex(0x241c17),
    Rgb::hex(0xfbf7ef),
    Rgb::hex(0xf7f9fb),
    Rgb::hex(0xfdf3f4),
    Rgb::hex(0xffffff),
];

pub const TEXT_PRESETS: [Rgb; 10] = [
    Rgb::hex(0xf2f3f7),
    Rgb::hex(0xffffff),
    Rgb::hex(0xf5efe6),
    Rgb::hex(0xc9ced8),
    Rgb::hex(0xdfe9fb),
    Rgb::hex(0x14161c),
    Rgb::hex(0x000000),
    Rgb::hex(0x2a2419),
    Rgb::hex(0x1c252c),
    Rgb::hex(0x2e3340),
];

/// Everything the style sheet asks for, worked out from a theme's colours.
#[derive(Debug, Clone, PartialEq)]
pub struct Palette {
    pub background: Rgb,
    pub card: Rgb,
    pub text: Rgb,
    pub accent: Rgb,
    pub light: bool,
}

impl Palette {
    /// `accent` is what it should be on a dark and on a light background.
    pub fn new(colours: CustomTheme, accent: (Rgb, Rgb)) -> Self {
        let background = colours.background;
        let light = background.is_light();
        let card = colours.card.unwrap_or_else(|| {
            if light {
                // A white page has no room left upward.
                if background.contrast(WHITE) < 1.05 {
                    background.mix(BLACK, 0.04)
                } else {
                    background.mix(WHITE, 0.6)
                }
            } else {
                background.mix(WHITE, 0.07)
            }
        });
        let text = colours
            .text
            .unwrap_or(if light { LIGHT.text } else { DARK.text })
            .readable_on(background, card);
        let wanted_accent = if light { accent.1 } else { accent.0 };
        Self {
            background,
            card,
            text,
            accent: wanted_accent.readable_on(background, card),
            light,
        }
    }

    pub fn from_base(base: BaseColours, accent: (Rgb, Rgb)) -> Self {
        Self::new(
            CustomTheme {
                background: base.background,
                card: Some(base.card),
                text: Some(base.text),
            },
            accent,
        )
    }

    /// The colour of the system bars: opaque, as Android wants it.
    pub fn background_argb(&self) -> u32 {
        0xFF00_0000
            | (u32::from(self.background.r) << 16)
            | (u32::from(self.background.g) << 8)
            | u32::from(self.background.b)
    }

    /// What goes on top of the accent colour.
    fn on_accent(&self) -> Rgb {
        if self.accent.is_light() {
            self.accent.mix(BLACK, 0.85)
        } else {
            WHITE
        }
    }

    /// The style sheet's colours, for the `style` of the page's root.
    pub fn css_vars(&self) -> String {
        let (bg, card, text, accent) = (self.background, self.card, self.text, self.accent);
        // A light card needs more of the text colour in it to look any different.
        let (raised, line, off, soft) = if self.light {
            (0.09, 0.12, 0.23, 0.12)
        } else {
            (0.06, 0.09, 0.145, 0.17)
        };
        // The colours that mean something keep their meaning, in a shade that reads here.
        let (warn, danger, ok) = if self.light {
            (Rgb::hex(0x8f5400), Rgb::hex(0xb3261e), Rgb::hex(0x1b7a4b))
        } else {
            (Rgb::hex(0xffb454), Rgb::hex(0xff8a7a), Rgb::hex(0x6fd6a0))
        };
        let (warn, danger, ok) = (
            warn.readable_on(bg, card),
            danger.readable_on(bg, card),
            ok.readable_on(bg, card),
        );
        let second_text = text.mix(bg, 0.3).readable_on(bg, card);
        let glow = |colour: Rgb, alpha: f32| {
            format!("rgba({}, {}, {}, {alpha})", colour.r, colour.g, colour.b)
        };
        let accent_glow = if self.light { 0.35 } else { 0.45 };
        let knob_off = if self.light { WHITE } else { second_text };
        format!(
            "--bg: {}; --surface: {}; --surface2: {}; --line: {}; --text: {}; --text2: {}; \
             --accent: {}; --on-accent: {}; --accent-soft: {}; --accent-glow: {}; --warn: {}; \
             --warn-soft: {}; --danger: {}; --ok: {}; --ok-glow: {}; --off: {}; --knob-off: {}; \
             --inverse: {}; --on-inverse: {}; --bar-soft: {};",
            bg.to_css(),
            card.to_css(),
            card.mix(text, raised).to_css(),
            card.mix(text, line).to_css(),
            text.to_css(),
            second_text.to_css(),
            accent.to_css(),
            self.on_accent().to_css(),
            bg.mix(accent, soft).to_css(),
            glow(accent, accent_glow),
            warn.to_css(),
            bg.mix(warn, 0.15).to_css(),
            danger.to_css(),
            ok.to_css(),
            glow(ok, 0.55),
            card.mix(text, off).to_css(),
            knob_off.to_css(),
            text.to_css(),
            card.to_css(),
            card.mix(accent, 0.45).to_css(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BLUE: (Rgb, Rgb) = (Rgb::hex(0x7cb0ff), Rgb::hex(0x1f5fd6));

    #[test]
    fn a_colour_reads_back_as_it_was_typed() {
        assert_eq!(Rgb::parse("#7cb0ff"), Some(Rgb::hex(0x7cb0ff)));
        assert_eq!(Rgb::parse("7CB0FF"), Some(Rgb::hex(0x7cb0ff)));
        assert_eq!(Rgb::parse(" #fff "), Some(WHITE));
        assert_eq!(Rgb::parse("#12345"), None);
        assert_eq!(Rgb::parse("#gggggg"), None);
        assert_eq!(Rgb::parse("é1234"), None);
        assert_eq!(Rgb::parse(""), None);
        assert_eq!(Rgb::hex(0x7cb0ff).to_css(), "#7cb0ff");
    }

    #[test]
    fn colours_are_saved_as_text() {
        let json = serde_json::to_string(&Rgb::hex(0x102030)).unwrap();
        assert_eq!(json, "\"#102030\"");
        assert_eq!(
            serde_json::from_str::<Rgb>(&json).unwrap(),
            Rgb::hex(0x102030)
        );
        assert!(serde_json::from_str::<Rgb>("\"nope\"").is_err());
    }

    #[test]
    fn the_wheel_gives_back_the_colour_it_was_set_to() {
        for colour in [
            Rgb::hex(0x7cb0ff),
            Rgb::hex(0xff0000),
            Rgb::hex(0x00ff00),
            Rgb::hex(0x0000ff),
            Rgb::hex(0xffc45c),
            Rgb::hex(0xb8246f),
            Rgb::hex(0x808080),
            Rgb::hex(0x000000),
            Rgb::hex(0xffffff),
        ] {
            let (hue, saturation, value) = colour.to_hsv();
            assert_eq!(Rgb::from_hsv(hue, saturation, value), colour);
        }
    }

    #[test]
    fn contrast_is_measured_as_usual() {
        assert!((WHITE.contrast(BLACK) - 21.0).abs() < 0.01);
        assert!((WHITE.contrast(WHITE) - 1.0).abs() < 0.01);
        assert!(WHITE.is_light());
        assert!(!BLACK.is_light());
    }

    #[test]
    fn a_colour_that_reads_stays_exactly_as_it_is() {
        let text = DARK.text;
        assert_eq!(text.readable_on(DARK.background, DARK.card), text);
    }

    #[test]
    fn black_on_black_cannot_be_chosen() {
        let palette = Palette::new(
            CustomTheme {
                background: BLACK,
                card: Some(BLACK),
                text: Some(BLACK),
            },
            (BLACK, BLACK),
        );
        assert!(palette.text.contrast(palette.background) >= MIN_CONTRAST);
        assert!(palette.accent.contrast(palette.background) >= MIN_CONTRAST);
    }

    #[test]
    fn text_reads_on_a_black_page_with_white_cards() {
        let palette = Palette::new(
            CustomTheme {
                background: BLACK,
                card: Some(WHITE),
                text: Some(WHITE),
            },
            BLUE,
        );
        assert!(palette.text.contrast(palette.background) >= MIN_CONTRAST);
        assert!(palette.text.contrast(palette.card) >= MIN_CONTRAST);
    }

    #[test]
    fn any_background_gets_readable_text_and_cards_by_itself() {
        for red in (0u8..=255).step_by(51) {
            for green in (0u8..=255).step_by(51) {
                for blue in (0u8..=255).step_by(51) {
                    let palette = Palette::new(
                        CustomTheme {
                            background: Rgb::new(red, green, blue),
                            card: None,
                            text: None,
                        },
                        BLUE,
                    );
                    let background = palette.background;
                    assert!(
                        palette.text.contrast(background) >= MIN_CONTRAST,
                        "text on {}",
                        background.to_css()
                    );
                    assert!(
                        palette.text.contrast(palette.card) >= MIN_CONTRAST,
                        "text on the cards of {}",
                        background.to_css()
                    );
                    assert!(
                        palette.accent.contrast(background) >= MIN_CONTRAST,
                        "accent on {}",
                        background.to_css()
                    );
                }
            }
        }
    }

    #[test]
    fn a_white_page_still_has_cards_that_show() {
        let palette = Palette::new(
            CustomTheme {
                background: WHITE,
                card: None,
                text: None,
            },
            BLUE,
        );
        assert_ne!(palette.card, palette.background);
    }

    #[test]
    fn the_style_sheet_gets_every_colour_it_uses() {
        let vars = Palette::from_base(DARK, BLUE).css_vars();
        let sheet = include_str!("../../assets/style.css");
        // Set on the element they are for, by the challenges and the confetti.
        const SET_IN_PLACE: [&str; 10] = [
            "fx", "fy", "size", "hue", "dx", "dy", "delay", "ring-1", "ring-2", "ring-3",
        ];
        // A colour the sheet uses and nothing sets would be invisible.
        for used in sheet.split("var(--").skip(1) {
            let reference = used.split(')').next().unwrap_or_default();
            // One with something to fall back on is fine without.
            if reference.contains(',') || SET_IN_PLACE.contains(&reference) {
                continue;
            }
            assert!(
                sheet.contains(&format!("--{reference}:"))
                    || vars.contains(&format!("--{reference}:")),
                "--{reference} is set nowhere"
            );
        }
    }

    #[test]
    fn the_themes_that_come_with_the_app_keep_their_look() {
        let vars = Palette::from_base(DARK, BLUE).css_vars();
        for exact in [
            "--bg: #101218;",
            "--surface: #1b1e27;",
            "--text: #f2f3f7;",
            "--accent: #7cb0ff;",
        ] {
            assert!(vars.contains(exact), "{exact} in {vars}");
        }
        let light = Palette::from_base(LIGHT, BLUE);
        assert!(light.light);
        assert!(light.css_vars().contains("--accent: #1f5fd6;"));
        assert!(!Palette::from_base(BLACK_THEME, BLUE).light);
        assert_eq!(
            Palette::from_base(LIGHT, BLUE).background_argb(),
            0xFFF5_F6F9
        );
        assert_eq!(
            Palette::from_base(BLACK_THEME, BLUE).background_argb(),
            0xFF00_0000
        );
    }
}
