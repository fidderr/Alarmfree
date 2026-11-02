//! Practice mode: run challenges without an alarm.
//!
//! Nothing rings, nothing is locked, no history is written and AlarmKit is
//! never called; the challenges are the very ones that would run when the
//! alarm fires (same generator, same components), so the user can try what
//! they set up. Exit is always one tap away.
//!
//! [`ChallengeRunner`] is the reusable part: the alarm editor shows it in
//! place to test the challenge being configured (leaving the editor would
//! lose unsaved edits), and the [`Practice`] route shows it for a saved alarm.

use crate::app::{t, AlarmData, Translations};
use crate::back::use_back_handler;
use crate::models::{AppSettings, ChallengeSetup};
use crate::screens::challenge_run::{prepare, ChallengeRun};
use dioxus::prelude::*;

/// Practice route: the challenges of a saved alarm.
#[component]
pub fn Practice(id: String) -> Element {
    let alarms = use_context::<Signal<Vec<AlarmData>>>();
    let nav = use_navigator();

    let list = alarms
        .read()
        .iter()
        .find(|a| a.id == id)
        .map(|a| ChallengeSetup::list_from_stored(&a.challenges))
        .unwrap_or_default();

    rsx! {
        ChallengeRunner {
            challenges: list,
            on_exit: move |_| {
                nav.go_back();
            },
        }
    }
}

/// Runs the given challenges one after another, as an alarm would, and calls `on_exit`
/// when the user leaves or has finished.
#[component]
pub fn ChallengeRunner(challenges: Vec<ChallengeSetup>, on_exit: EventHandler<()>) -> Element {
    let translations = use_context::<Translations>();
    use_back_handler(on_exit);

    let settings = use_context::<Signal<AppSettings>>();
    let prepared = use_signal(|| prepare(&challenges, false, settings.peek().locale));
    let mut finished = use_signal(|| false);

    let exit_text = t(&translations, "practice.exit");
    let done_text = t(&translations, "practice.done");
    let empty_text = t(&translations, "practice.empty");

    if finished() || prepared.read().is_empty() {
        let message = if finished() { done_text } else { empty_text };
        return rsx! {
            div { class: "screen practice-done",
                p { class: "practice-message", "{message}" }
                button { class: "btn btn-primary", onclick: move |_| on_exit.call(()), "{exit_text}" }
            }
        };
    }

    rsx! {
        ChallengeRun {
            challenges: prepared(),
            on_solved: move |_| finished.set(true),
            on_exit,
        }
    }
}
