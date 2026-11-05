//! How the screen looks while this alarm rings. Only the draft changes here, like everywhere in
//! the editor: it counts when the alarm is saved.

use dioxus::prelude::*;

use super::{EditorSheet, EditorState};
use crate::app::{t, Translations};
use crate::back::use_back_handler;
use crate::components::{
    Chips, ColorControls, GroupTitle, Icon, IconName, Sheet, SheetDone, ToastMessage,
};
use crate::models::{Accent, AppSettings, RingBackdrop, RingClock};
use crate::screens::ring_face::{ring_date, RingBackdropView, RingFace};

/// The link top right that shows it full screen.
#[component]
fn PreviewLink() -> Element {
    let EditorState {
        mut previewing_look,
        ..
    } = use_context();
    let translations = use_context::<Translations>();
    rsx! {
        button {
            class: "link-btn look-preview-link",
            onclick: move |_| previewing_look.set(true),
            {t(&translations, "ring.preview")}
        }
    }
}

/// The background, its colour and the style of the clock.
#[component]
pub(super) fn RingLookSheet() -> Element {
    let EditorState {
        mut sheet,
        mut draft,
        ..
    } = use_context();
    let translations = use_context::<Translations>();
    let mut toast = use_context::<Signal<Option<ToastMessage>>>();
    let settings = use_context::<Signal<AppSettings>>();
    let look = draft.read().ring_look.clone();
    let accent = settings
        .read()
        .palette(mobile_sentinel::display::system_dark())
        .accent;

    let pick_photo = use_callback(move |_: ()| {
        spawn(async move {
            let (tx, rx) = futures_channel::oneshot::channel();
            std::thread::spawn(move || {
                let _ = tx.send(crate::ring_photos::import());
            });
            match rx.await {
                Ok(Ok(Some(name))) => {
                    let mut alarm = draft.write();
                    alarm.ring_look.photo = Some(name);
                    alarm.ring_look.backdrop = RingBackdrop::Photo;
                }
                Ok(Err(reason)) => {
                    log::warn!("[AlarmFree] No photo imported: {reason}");
                    toast.set(Some(ToastMessage::error(t(
                        &translations,
                        "ring.photo_failed",
                    ))));
                }
                // The user changed their mind.
                Ok(Ok(None)) | Err(_) => {}
            }
        });
    });

    rsx! {
        Sheet {
            title: t(&translations, "config.ring_look"),
            help: t(&translations, "config.ring_look_note"),
            action: rsx! { PreviewLink {} },
            on_close: move |_| sheet.set(None),
            div { class: "look-grid",
                for backdrop in RingBackdrop::ALL {
                    {
                        let selected = look.backdrop == backdrop;
                        let mut shown = look.clone();
                        shown.backdrop = backdrop;
                        let no_photo_yet = backdrop == RingBackdrop::Photo && look.photo.is_none();
                        rsx! {
                            button {
                                key: "{backdrop:?}",
                                class: if selected { "look-tile selected" } else { "look-tile" },
                                "aria-pressed": if selected { "true" } else { "false" },
                                onclick: move |_| {
                                    if no_photo_yet {
                                        pick_photo.call(());
                                    } else {
                                        draft.write().ring_look.backdrop = backdrop;
                                    }
                                },
                                span { class: "look-tile-face",
                                    RingBackdropView { look: shown }
                                    if no_photo_yet {
                                        span { class: "look-tile-add",
                                            Icon { name: IconName::Plus, size: 20 }
                                        }
                                    }
                                }
                                span { class: "look-tile-name", {t(&translations, backdrop.name_key())} }
                            }
                        }
                    }
                }
            }
            if look.backdrop == RingBackdrop::Photo {
                button {
                    class: "link-btn look-other-photo",
                    onclick: move |_| pick_photo.call(()),
                    {t(&translations, "ring.photo_other")}
                }
            }
            if look.backdrop.takes_a_colour() {
                button {
                    class: "look-row",
                    onclick: move |_| sheet.set(Some(EditorSheet::RingColour)),
                    span { {t(&translations, "config.ring_colour")} }
                    span { class: "grow" }
                    if look.colour.is_none() {
                        span { class: "look-row-value", {t(&translations, "theme.auto")} }
                    }
                    span {
                        class: "color-dot",
                        style: "background-color: {look.colours(accent)[0].to_css()};",
                    }
                    Icon { name: IconName::ChevronRight, size: 16 }
                }
            }
            GroupTitle { text: t(&translations, "config.ring_clock") }
            Chips::<RingClock> {
                grow: true,
                options: RingClock::ALL.iter().map(|clock| (*clock, t(&translations, clock.name_key()))).collect::<Vec<_>>(),
                selected: Some(look.clock),
                on_pick: move |clock: RingClock| draft.write().ring_look.clock = clock,
            }
            SheetDone { label: t(&translations, "action.done"), on_click: move |_| sheet.set(None) }
        }
    }
}

/// The colour the background moves in: the same choice as the accent colour, and it shows what it
/// does while choosing.
#[component]
pub(super) fn RingColourSheet() -> Element {
    let EditorState {
        mut sheet,
        mut draft,
        ..
    } = use_context();
    let translations = use_context::<Translations>();
    let settings = use_context::<Signal<AppSettings>>();
    let palette = settings
        .read()
        .palette(mobile_sentinel::display::system_dark());
    let look = draft.read().ring_look.clone();
    let base = look.colours(palette.accent)[0];
    let mut hsv = use_signal(|| base.to_hsv());
    let mut typed: Signal<Option<String>> = use_signal(|| None);
    let code = typed().unwrap_or_else(|| base.to_css());
    let auto_on = look.colour.is_none();

    rsx! {
        Sheet {
            title: t(&translations, "config.ring_colour"),
            action: rsx! { PreviewLink {} },
            on_close: move |_| sheet.set(Some(EditorSheet::RingLook)),
            div { class: "look-live",
                RingBackdropView { look: look.clone() }
            }
            div { class: "swatches",
                for preset in Accent::PRESETS {
                    {
                        let colours = preset.colours();
                        let colour = if palette.light { colours.1 } else { colours.0 };
                        let selected = look.colour == Some(colour);
                        rsx! {
                            button {
                                key: "{preset:?}",
                                class: if selected { "swatch selected" } else { "swatch" },
                                style: "background-color: {colour.to_css()};",
                                "aria-label": t(&translations, preset.name_key()),
                                "aria-pressed": if selected { "true" } else { "false" },
                                onclick: move |_| {
                                    hsv.set(colour.to_hsv());
                                    typed.set(None);
                                    draft.write().ring_look.colour = Some(colour);
                                },
                            }
                        }
                    }
                }
            }
            ColorControls {
                hsv,
                typed,
                code,
                on_colour: move |colour| draft.write().ring_look.colour = Some(colour),
                on_release: move |_| {},
                extra: rsx! {
                    button {
                        class: if auto_on { "chip color-auto selected" } else { "chip color-auto" },
                        "aria-pressed": if auto_on { "true" } else { "false" },
                        onclick: move |_| {
                            typed.set(None);
                            draft.write().ring_look.colour = None;
                            let accent = settings
                                .peek()
                                .palette(mobile_sentinel::display::system_dark())
                                .accent;
                            hsv.set(draft.peek().ring_look.colours(accent)[0].to_hsv());
                        },
                        {t(&translations, "theme.auto")}
                    }
                },
            }
            SheetDone { label: t(&translations, "action.done"), on_click: move |_| sheet.set(Some(EditorSheet::RingLook)) }
        }
    }
}

/// The screen as it will look, without the sound. A tap anywhere closes it, also where the buttons
/// are: they do not work here, and must not seem to.
#[component]
pub(super) fn RingPreview() -> Element {
    let EditorState {
        draft,
        mut previewing_look,
        ..
    } = use_context();
    let translations = use_context::<Translations>();
    let settings = use_context::<Signal<AppSettings>>();
    use_back_handler(EventHandler::new(move |_| previewing_look.set(false)));

    let alarm = draft.read().clone();
    let twenty_four = crate::text::uses_24_hours(&settings.read());
    let clock = crate::text::clock(alarm.time_hour, alarm.time_minute, twenty_four);
    let label = if alarm.label.is_empty() {
        t(&translations, "alarm.fallback_label")
    } else {
        alarm.label.clone()
    };
    let snooze_text = t(&translations, "ring.snooze")
        .replace("{minutes}", &alarm.snooze_interval.to_string())
        .replace("{left}", &alarm.snooze_count.to_string());

    rsx! {
        div {
            class: "ring-preview",
            onclick: move |_| previewing_look.set(false),
            RingFace {
                look: alarm.ring_look.clone(),
                date: ring_date(&translations, chrono::Local::now()),
                clock,
                twenty_four,
                label,
                note: alarm.note.clone(),
                div { class: "grow" }
                if alarm.snooze_count > 0 {
                    div { class: "btn ring-snooze", "{snooze_text}" }
                }
                div { class: "ring-challenges",
                    span { {t(&translations, "ring.preview_hint")} }
                }
                div { class: "btn btn-primary ring-off", {t(&translations, "ring.turn_off")} }
            }
        }
    }
}
