use dioxus::prelude::*;

use crate::app::{t, Translations};

/// Math challenge component: displays expression and a numeric keypad.
/// Auto-submits when the typed answer equals the expected value.
/// Shows a warning colour if the user explicitly checks a wrong answer.
#[component]
pub fn MathChallenge(expression: String, expected: i64, on_answer: EventHandler<i64>) -> Element {
    let translations = use_context::<Translations>();
    let instruction = t(&translations, "challenge.instruction.math");
    let delete_text = t(&translations, "action.delete_digit");
    let check_text = t(&translations, "action.check");
    let mut input_value = use_signal(String::new);
    let mut wrong = use_signal(|| false);
    let mut already_submitted = use_signal(|| false);

    let current = input_value.read().clone();

    // Auto-submit when answer matches — but only ONCE per component instance.
    if let Ok(v) = current.parse::<i64>() {
        if v == expected && !*already_submitted.read() {
            already_submitted.set(true);
            on_answer.call(v);
        }
    }

    let submit = move |_| {
        if *already_submitted.read() {
            return;
        }
        match input_value.read().parse::<i64>() {
            Ok(v) if v == expected => {
                already_submitted.set(true);
                on_answer.call(v);
            }
            _ => wrong.set(true),
        }
    };

    let display_class = if *wrong.read() {
        "math-display wrong"
    } else {
        "math-display"
    };

    rsx! {
        div { class: "challenge-wrapper",
            p { class: "challenge-instruction", "{instruction}" }
            div { class: "math-expression", "{expression}" }
            div { class: "{display_class}", "aria-live": "polite", "{current}" }
            div { class: "challenge-space" }

            div { class: "math-keypad",
                for digit in ['1', '2', '3', '4', '5', '6', '7', '8', '9'] {
                    button {
                        class: "math-key",
                        onclick: move |_| {
                            let mut val = input_value.write();
                            if val.chars().count() < 12 {
                                val.push(digit);
                            }
                            drop(val);
                            wrong.set(false);
                        },
                        "{digit}"
                    }
                }
                button {
                    class: "math-key math-key-action",
                    "aria-label": "{delete_text}",
                    onclick: move |_| {
                        input_value.write().pop();
                        wrong.set(false);
                    },
                    "\u{232B}"
                }
                button {
                    class: "math-key",
                    onclick: move |_| {
                        let mut val = input_value.write();
                        if val.chars().count() < 12 {
                            val.push('0');
                        }
                        drop(val);
                        wrong.set(false);
                    },
                    "0"
                }
                button {
                    class: "math-key math-key-submit",
                    onclick: submit,
                    "{check_text}"
                }
            }
        }
    }
}
