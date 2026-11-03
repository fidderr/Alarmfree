use dioxus::prelude::*;

use crate::app::{t, Translations};

/// How long an answer can get.
const MAX_DIGITS: usize = 12;

/// The sum, and a keypad to answer it on. It is done as soon as the answer is there; the
/// check key is for wanting to know that what is typed is not it.
#[component]
pub fn MathChallenge(expression: String, expected: i64, on_solved: EventHandler<()>) -> Element {
    let translations = use_context::<Translations>();
    let instruction = t(&translations, "challenge.instruction.math");
    let delete_text = t(&translations, "action.delete_digit");
    let check_text = t(&translations, "action.check");
    let incorrect_text = t(&translations, "challenge.incorrect");
    let mut typed = use_signal(String::new);
    let mut wrong = use_signal(|| false);
    let mut solved = use_signal(|| false);

    let mut press = move |digit: char| {
        if solved() {
            return;
        }
        wrong.set(false);
        if typed.read().chars().count() < MAX_DIGITS {
            typed.write().push(digit);
        }
        if typed.read().parse::<i64>() == Ok(expected) {
            solved.set(true);
            on_solved.call(());
        }
    };

    rsx! {
        div { class: "challenge-wrapper",
            p { class: "challenge-instruction", "{instruction}" }
            div { class: "math-expression", "{expression}" }
            div {
                class: if wrong() { "math-display wrong" } else { "math-display" },
                "aria-live": "polite",
                "{typed}"
            }
            if wrong() {
                p { class: "run-wrong", role: "status", "{incorrect_text}" }
            }
            div { class: "challenge-space" }

            div { class: "math-keypad",
                for digit in ['1', '2', '3', '4', '5', '6', '7', '8', '9'] {
                    button {
                        class: "math-key",
                        onclick: move |_| press(digit),
                        "{digit}"
                    }
                }
                button {
                    class: "math-key math-key-action",
                    "aria-label": "{delete_text}",
                    onclick: move |_| {
                        typed.write().pop();
                        wrong.set(false);
                    },
                    "\u{232B}"
                }
                button {
                    class: "math-key",
                    onclick: move |_| press('0'),
                    "0"
                }
                button {
                    class: "math-key math-key-submit",
                    onclick: move |_| wrong.set(true),
                    "{check_text}"
                }
            }
        }
    }
}
