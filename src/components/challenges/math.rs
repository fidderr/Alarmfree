use dioxus::prelude::*;

use crate::app::{t, Translations};
use crate::components::{Icon, IconName};

/// Math challenge component: displays expression and a numeric keypad.
/// Auto-submits when the typed answer equals the expected value.
/// Shows red outline if the user explicitly hits the check button with a wrong answer.
#[component]
pub fn MathChallenge(expression: String, expected: i64, on_answer: EventHandler<i64>) -> Element {
    let translations = use_context::<Translations>();
    let instruction = t(&translations, "challenge.instruction.math");
    let mut input_value = use_signal(String::new);
    let mut wrong = use_signal(|| false);
    let mut already_submitted = use_signal(|| false);

    let current = input_value.read().clone();

    // Parse the current input as i64 (handles leading minus)
    let parsed: Option<i64> = current.parse::<i64>().ok();

    // Auto-submit when answer matches — but only ONCE per component instance.
    if let Some(v) = parsed {
        if v == expected && !*already_submitted.read() {
            already_submitted.set(true);
            on_answer.call(v);
        }
    }

    let submit = move |_| {
        if *already_submitted.read() {
            return;
        }
        if let Ok(v) = input_value.read().parse::<i64>() {
            if v == expected {
                already_submitted.set(true);
                on_answer.call(v);
            } else {
                wrong.set(true);
            }
        } else {
            wrong.set(true);
        }
    };

    let display = if current.is_empty() {
        "0".to_string()
    } else {
        current.clone()
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

            div { class: "{display_class}", "{display}" }

            div { class: "math-keypad",
                // Row 1: 1 2 3
                button { class: "math-key", onclick: move |_| {
                    let mut val = input_value.write();
                    if val.chars().count() < 12 { val.push('1'); }
                    drop(val);
                    wrong.set(false);
                }, "1" }
                button { class: "math-key", onclick: move |_| {
                    let mut val = input_value.write();
                    if val.chars().count() < 12 { val.push('2'); }
                    drop(val);
                    wrong.set(false);
                }, "2" }
                button { class: "math-key", onclick: move |_| {
                    let mut val = input_value.write();
                    if val.chars().count() < 12 { val.push('3'); }
                    drop(val);
                    wrong.set(false);
                }, "3" }
                button { class: "math-key", onclick: move |_| {
                    let mut val = input_value.write();
                    if val.chars().count() < 12 { val.push('4'); }
                    drop(val);
                    wrong.set(false);
                }, "4" }
                button { class: "math-key", onclick: move |_| {
                    let mut val = input_value.write();
                    if val.chars().count() < 12 { val.push('5'); }
                    drop(val);
                    wrong.set(false);
                }, "5" }
                button { class: "math-key", onclick: move |_| {
                    let mut val = input_value.write();
                    if val.chars().count() < 12 { val.push('6'); }
                    drop(val);
                    wrong.set(false);
                }, "6" }
                button { class: "math-key", onclick: move |_| {
                    let mut val = input_value.write();
                    if val.chars().count() < 12 { val.push('7'); }
                    drop(val);
                    wrong.set(false);
                }, "7" }
                button { class: "math-key", onclick: move |_| {
                    let mut val = input_value.write();
                    if val.chars().count() < 12 { val.push('8'); }
                    drop(val);
                    wrong.set(false);
                }, "8" }
                button { class: "math-key", onclick: move |_| {
                    let mut val = input_value.write();
                    if val.chars().count() < 12 { val.push('9'); }
                    drop(val);
                    wrong.set(false);
                }, "9" }
                // Row 4: ± 0 ⌫
                button { class: "math-key math-key-action", onclick: move |_| {
                    let mut val = input_value.write();
                    if val.starts_with('-') {
                        val.remove(0);
                    } else if !val.is_empty() {
                        val.insert(0, '-');
                    }
                    drop(val);
                    wrong.set(false);
                }, "±" }
                button { class: "math-key", onclick: move |_| {
                    let mut val = input_value.write();
                    if val.chars().count() < 12 { val.push('0'); }
                    drop(val);
                    wrong.set(false);
                }, "0" }
                button {
                    class: "math-key math-key-action",
                    onclick: move |_| {
                        let mut val = input_value.write();
                        val.pop();
                        drop(val);
                        wrong.set(false);
                    },
                    Icon { name: IconName::ArrowLeft, size: 20 }
                }
                // Row 5: full-width check button
                button {
                    class: "math-key math-key-submit",
                    onclick: submit,
                    Icon { name: IconName::Check, size: 22 }
                }
            }
        }
    }
}
