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
use crate::components::{Celebration, Icon, IconName};
use crate::screens::alarm_firing::{generate_challenges, render_challenge, RateLimiter};
use dioxus::prelude::*;

/// Practice route: the challenges of a saved alarm.
#[component]
pub fn Practice(id: String) -> Element {
    let alarms = use_context::<Signal<Vec<AlarmData>>>();
    let nav = use_navigator();

    let list: Vec<(String, String)> = alarms
        .read()
        .iter()
        .find(|a| a.id == id)
        .map(|a| a.challenges.clone())
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

/// Runs the given `(type, config_json)` challenges one after another, as an
/// alarm would, and calls `on_exit` when the user leaves or has finished.
#[component]
pub fn ChallengeRunner(challenges: Vec<(String, String)>, on_exit: EventHandler<()>) -> Element {
    let translations = use_context::<Translations>();

    let generated = use_signal(|| generate_challenges(&challenges));
    let current_challenge_index = use_signal(|| 0usize);
    let answer_feedback = use_signal(|| "");
    let rate_limiter = use_signal(RateLimiter::new);
    let mut challenge_timeout = use_signal(|| 0u32);
    let celebrating = use_signal(|| false);
    let challenge_solves = use_signal(Vec::new);
    let challenge_started_at = use_signal(|| None);
    let using_fallback = use_signal(|| false);
    let fallback_armed = use_signal(|| false);
    let mut finished = use_signal(|| false);

    // The rate limiter's cooldown is counted in these ticks.
    use_future(move || async move {
        loop {
            crate::timer::sleep_ms(1000).await;
            challenge_timeout += 1;
        }
    });

    let exit_text = t(&translations, "practice.exit");
    let title_text = t(&translations, "practice.title");
    let done_text = t(&translations, "practice.done");
    let empty_text = t(&translations, "practice.empty");
    let unknown_text = t(&translations, "challenge.unknown_type");
    let incorrect_text = t(&translations, "challenge.incorrect");

    let total = generated.read().len();
    let idx = *current_challenge_index.read();
    let current = generated.read().get(idx).cloned();

    rsx! {
        div { class: "screen alarm-firing-screen in-challenge practice-screen",
            div { class: "practice-bar",
                button {
                    class: "back-btn",
                    onclick: move |_| on_exit.call(()),
                    Icon { name: IconName::ArrowLeft, size: 16 }
                    " {exit_text}"
                }
                span { class: "practice-title", "{title_text}" }
                if total > 1 {
                    span { class: "challenge-pill challenge-pill-progress", "{idx + 1} / {total}" }
                }
            }

            if finished() {
                div { class: "practice-done",
                    p { "{done_text}" }
                    button {
                        class: "btn-primary",
                        onclick: move |_| on_exit.call(()),
                        "{exit_text}"
                    }
                }
            } else if let Some(challenge) = current {
                div { class: "challenge-area",
                    {
                        render_challenge(
                            challenge,
                            idx,
                            total,
                            current_challenge_index,
                            answer_feedback,
                            EventHandler::new(move |_| finished.set(true)),
                            rate_limiter,
                            challenge_timeout,
                            celebrating,
                            challenge_solves,
                            challenge_started_at,
                            using_fallback,
                            fallback_armed,
                            // Empty: practice, there is no alarm to dismiss.
                            String::new(),
                            unknown_text,
                        )
                    }
                    if *celebrating.read() {
                        Celebration { label: String::new() }
                    }
                    if *answer_feedback.read() == "wrong" {
                        div { class: "answer-feedback-toast", "{incorrect_text}" }
                    }
                }
            } else {
                div { class: "practice-done",
                    p { "{empty_text}" }
                    button {
                        class: "btn-primary",
                        onclick: move |_| on_exit.call(()),
                        "{exit_text}"
                    }
                }
            }
        }
    }
}
