//! How the screen looks while an alarm rings: a background, the colour it moves in, and the
//! clock's style. All of it is decoration. What cannot be read or shown is the plain screen, and
//! none of it has anything to do with the ring itself.

use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer, Serialize};

use super::theme::Rgb;

/// What moves behind the time.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum RingBackdrop {
    #[default]
    Plain,
    Aurora,
    Sunrise,
    Waves,
    Stars,
    Rings,
    Bubbles,
    Photo,
}

impl RingBackdrop {
    /// In the order the panel shows them.
    pub const ALL: [Self; 8] = [
        Self::Plain,
        Self::Aurora,
        Self::Sunrise,
        Self::Waves,
        Self::Stars,
        Self::Rings,
        Self::Bubbles,
        Self::Photo,
    ];

    pub fn name_key(self) -> &'static str {
        match self {
            Self::Plain => "ring.backdrop.plain",
            Self::Aurora => "ring.backdrop.aurora",
            Self::Sunrise => "ring.backdrop.sunrise",
            Self::Waves => "ring.backdrop.waves",
            Self::Stars => "ring.backdrop.stars",
            Self::Rings => "ring.backdrop.rings",
            Self::Bubbles => "ring.backdrop.bubbles",
            Self::Photo => "ring.backdrop.photo",
        }
    }

    /// Its class in the style sheet, for the ones the style sheet draws.
    pub fn class(self) -> &'static str {
        match self {
            Self::Plain | Self::Photo => "",
            Self::Aurora => "backdrop-aurora",
            Self::Sunrise => "backdrop-sunrise",
            Self::Waves => "backdrop-waves",
            Self::Stars => "backdrop-stars",
            Self::Rings => "backdrop-rings",
            Self::Bubbles => "backdrop-bubbles",
        }
    }

    /// How many moving parts the style sheet expects.
    pub fn parts(self) -> usize {
        match self {
            Self::Plain | Self::Photo => 0,
            Self::Sunrise => 2,
            Self::Aurora | Self::Waves | Self::Stars => 3,
            Self::Rings => 4,
            Self::Bubbles => 9,
        }
    }

    /// Whether a colour means anything for it.
    pub fn takes_a_colour(self) -> bool {
        !matches!(self, Self::Plain | Self::Photo)
    }
}

/// The style of the big clock.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum RingClock {
    #[default]
    Clean,
    Thin,
    Bold,
    Classic,
    Mono,
}

impl RingClock {
    pub const ALL: [Self; 5] = [
        Self::Clean,
        Self::Thin,
        Self::Bold,
        Self::Classic,
        Self::Mono,
    ];

    pub fn name_key(self) -> &'static str {
        match self {
            Self::Clean => "ring.clock.clean",
            Self::Thin => "ring.clock.thin",
            Self::Bold => "ring.clock.bold",
            Self::Classic => "ring.clock.classic",
            Self::Mono => "ring.clock.mono",
        }
    }

    pub fn class(self) -> &'static str {
        match self {
            Self::Clean => "",
            Self::Thin => " clock-thin",
            Self::Bold => " clock-bold",
            Self::Classic => " clock-classic",
            Self::Mono => " clock-mono",
        }
    }
}

/// Warm by itself: a sunrise in the accent colour of the day is a strange sunrise.
const SUNRISE: Rgb = Rgb::new(0xff, 0x8a, 0x4c);

/// How far apart on the wheel the colours of a background are.
const STEP: f32 = 35.0;

/// The look of one alarm's ring screen.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct RingLook {
    #[serde(default, deserialize_with = "or_default")]
    pub backdrop: RingBackdrop,
    /// The colour it moves in. `None` follows the accent colour of the theme.
    #[serde(default, deserialize_with = "or_default")]
    pub colour: Option<Rgb>,
    #[serde(default, deserialize_with = "or_default")]
    pub clock: RingClock,
    /// A file in the app's own files, see `ring_photos`. Stays when another background is picked,
    /// so going back to it needs no second search.
    #[serde(default, deserialize_with = "or_default")]
    pub photo: Option<String>,
}

/// A backup from a later version can say what this one does not know. An alarm must never get
/// lost over how it looks.
fn or_default<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: Default + DeserializeOwned,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    Ok(serde_json::from_value(value).unwrap_or_default())
}

impl RingLook {
    /// The colour it moves in, and two neighbours on the wheel: more alive than one colour, and
    /// they stay together whatever is picked. Yellow and yellow-green turn to olive when they are
    /// faint on a dark screen, so a neighbour that lands there goes the other way round. A sunrise
    /// is the exception: its yellow is the bright middle of the sun.
    pub fn colours(&self, accent: Rgb) -> [Rgb; 3] {
        let sunrise = self.backdrop == RingBackdrop::Sunrise;
        let base = self
            .colour
            .unwrap_or(if sunrise { SUNRISE } else { accent });
        let (hue, saturation, value) = base.to_hsv();
        let olive = |by: f32| !sunrise && (50.0..=100.0).contains(&(hue + by).rem_euclid(360.0));
        let (one, two) = if olive(STEP) {
            (-STEP, -2.0 * STEP)
        } else if olive(-STEP) {
            (STEP, 2.0 * STEP)
        } else {
            (STEP, -STEP)
        };
        let turned = |by: f32| Rgb::from_hsv((hue + by).rem_euclid(360.0), saturation, value);
        [base, turned(one), turned(two)]
    }

    /// For the `style` of the background.
    pub fn css_vars(&self, accent: Rgb) -> String {
        let [one, two, three] = self.colours(accent);
        format!(
            "--ring-1: {}; --ring-2: {}; --ring-3: {};",
            one.to_css(),
            two.to_css(),
            three.to_css()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_is_the_plain_screen_until_someone_says_otherwise() {
        let look: RingLook = serde_json::from_str("{}").unwrap();
        assert_eq!(look, RingLook::default());
        assert_eq!(look.backdrop, RingBackdrop::Plain);
        assert_eq!(look.clock, RingClock::Clean);
    }

    #[test]
    fn what_this_version_does_not_know_loses_nothing_else() {
        let look: RingLook = serde_json::from_str(
            r##"{"backdrop":"Fireworks","colour":"not a colour","clock":"Waves","photo":"a.jpg"}"##,
        )
        .unwrap();
        assert_eq!(look.backdrop, RingBackdrop::Plain);
        assert_eq!(look.colour, None);
        assert_eq!(look.clock, RingClock::Clean);
        assert_eq!(look.photo.as_deref(), Some("a.jpg"));
    }

    #[test]
    fn it_comes_back_as_it_was_saved() {
        let look = RingLook {
            backdrop: RingBackdrop::Waves,
            colour: Some(Rgb::new(0x10, 0x20, 0x30)),
            clock: RingClock::Mono,
            photo: Some("a.jpg".to_owned()),
        };
        let again = serde_json::to_string(&look).unwrap();
        assert_eq!(serde_json::from_str::<RingLook>(&again).unwrap(), look);
    }

    #[test]
    fn it_follows_the_accent_until_it_has_a_colour_of_its_own() {
        let accent = Rgb::new(0x7c, 0xb0, 0xff);
        let mut look = RingLook {
            backdrop: RingBackdrop::Aurora,
            ..RingLook::default()
        };
        assert_eq!(look.colours(accent)[0], accent);
        let own = Rgb::new(0xc0, 0x60, 0x20);
        look.colour = Some(own);
        assert_eq!(look.colours(accent)[0], own);
        assert_ne!(look.colours(accent)[1], own, "its neighbours differ");
    }

    #[test]
    fn a_sunrise_is_warm_by_itself() {
        let look = RingLook {
            backdrop: RingBackdrop::Sunrise,
            ..RingLook::default()
        };
        assert_eq!(look.colours(Rgb::new(0, 0, 0xff))[0], SUNRISE);
    }

    #[test]
    fn a_warm_colour_gets_warm_neighbours_and_no_olive() {
        let look = RingLook {
            backdrop: RingBackdrop::Waves,
            ..RingLook::default()
        };
        for accent in [
            Rgb::new(0xff, 0xc4, 0x5c),
            Rgb::new(0x8a, 0x5a, 0x00),
            Rgb::new(0x9b, 0xd1, 0x9b),
            Rgb::new(0x7c, 0xb0, 0xff),
            Rgb::new(0xff, 0x94, 0xc8),
        ] {
            let [_, one, two] = look.colours(accent);
            for neighbour in [one, two] {
                let (hue, _, _) = neighbour.to_hsv();
                assert!(
                    !(50.0..=100.0).contains(&hue),
                    "{} gives {}",
                    accent.to_css(),
                    neighbour.to_css()
                );
            }
        }
    }

    #[test]
    fn every_background_has_a_name_and_every_moving_one_a_class() {
        for backdrop in RingBackdrop::ALL {
            assert!(!backdrop.name_key().is_empty());
            assert_eq!(backdrop.takes_a_colour(), !backdrop.class().is_empty());
            assert_eq!(backdrop.takes_a_colour(), backdrop.parts() > 0);
        }
    }
}
