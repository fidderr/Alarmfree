//! What a ringing alarm looks like, in the one place: the ring screen and its preview show the
//! same thing. The background is decoration behind the rest. It takes no touches, and what it
//! cannot show leaves the plain screen.

use chrono::{DateTime, Datelike, Local};
use dioxus::prelude::*;

use crate::app::{t, Translations};
use crate::models::{AppSettings, RingBackdrop, RingLook};

/// "Thursday 5 Nov".
pub fn ring_date(translations: &Translations, moment: DateTime<Local>) -> String {
    format!(
        "{} {} {}",
        t(
            translations,
            crate::text::FULL_DAY_KEYS[moment.weekday().num_days_from_monday() as usize]
        ),
        moment.day(),
        crate::text::month_short(translations, moment.month0() as usize),
    )
}

/// The photo of this look, if that is what it shows and it can be read.
fn photo_of(look: &RingLook) -> Option<String> {
    if look.backdrop != RingBackdrop::Photo {
        return None;
    }
    look.photo.as_deref().and_then(crate::ring_photos::data_url)
}

/// What moves behind the time. Also what a tile in the panel shows, smaller.
#[component]
pub fn RingBackdropView(look: RingLook) -> Element {
    let settings = use_context::<Signal<AppSettings>>();
    let accent = settings
        .read()
        .palette(mobile_sentinel::display::system_dark())
        .accent;
    if let Some(photo) = photo_of(&look) {
        return rsx! {
            div { class: "ring-backdrop", "aria-hidden": "true",
                img { class: "ring-photo", src: "{photo}", alt: "" }
                span { class: "ring-photo-scrim" }
            }
        };
    }
    if !look.backdrop.takes_a_colour() {
        return rsx! {};
    }
    let class = format!("ring-backdrop {}", look.backdrop.class());
    let style = look.css_vars(accent);
    rsx! {
        div { class: "{class}", style: "{style}", "aria-hidden": "true",
            for _ in 0..look.backdrop.parts() {
                span {}
            }
        }
    }
}

/// The screen of a ringing alarm: the time, what it is for, and below that whatever is passed in.
#[component]
pub fn RingFace(
    look: RingLook,
    date: String,
    clock: String,
    twenty_four: bool,
    label: String,
    note: String,
    children: Element,
) -> Element {
    let on_photo = photo_of(&look).is_some();
    let dressed = on_photo || look.backdrop.takes_a_colour();
    let screen_class = format!(
        "screen ring-screen{}{}",
        if dressed { " ring-dressed" } else { "" },
        if on_photo { " ring-on-photo" } else { "" },
    );
    let clock_class = format!(
        "ring-clock{}{}",
        if twenty_four { "" } else { " h12" },
        look.clock.class(),
    );
    rsx! {
        div { class: "{screen_class}",
            RingBackdropView { look }
            p { class: "ring-date", "{date}" }
            p { class: "{clock_class}", "{clock}" }
            h1 { class: "ring-label", "{label}" }
            if !note.is_empty() {
                p { class: "ring-note", "{note}" }
            }
            {children}
        }
    }
}
