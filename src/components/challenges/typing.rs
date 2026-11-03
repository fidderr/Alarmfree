use dioxus::prelude::*;

use crate::app::{t, Translations};

/// Typing challenge: displays passage with character-by-character highlighting.
/// Correct chars turn green, wrong chars turn red. Auto-submits when complete.
/// Comparison is case-insensitive. Words wrap as whole units (no mid-word breaks).
#[component]
pub fn TypingChallenge(
    passage: String,
    min_accuracy: f32,
    on_complete: EventHandler<()>,
) -> Element {
    let translations = use_context::<Translations>();
    let instruction = t(&translations, "challenge.instruction.typing");
    let placeholder = t(&translations, "challenge.typing.placeholder");
    let incorrect_text = t(&translations, "challenge.incorrect");
    let mut typed_text = use_signal(String::new);
    let mut wrong = use_signal(|| false);

    let typed_chars: Vec<char> = typed_text.read().chars().collect();
    let typed_len = typed_chars.len();
    let passage_len = passage.chars().count();

    // So far: against as much of the passage as has been typed.
    let passage_chars: Vec<char> = passage.chars().collect();
    let passage_so_far: String = passage_chars.iter().take(typed_len).collect();
    let accuracy = crate::models::typing_accuracy(&passage_so_far, &typed_text.read());
    let accuracy_pct = (accuracy * 100.0) as u32;

    let accuracy_class = if accuracy >= min_accuracy {
        "good"
    } else {
        "bad"
    };
    let min_accuracy_pct = (min_accuracy * 100.0) as u32;

    // Split passage into words (with their starting indices) so each word
    // can render as a single inline-block unit that won't break mid-word.
    let mut words: Vec<(usize, String)> = Vec::new();
    {
        let mut current = String::new();
        let mut start = 0usize;
        for (i, ch) in passage_chars.iter().enumerate() {
            if *ch == ' ' {
                if !current.is_empty() {
                    words.push((start, current.clone()));
                    current.clear();
                }
                start = i + 1;
            } else {
                if current.is_empty() {
                    start = i;
                }
                current.push(*ch);
            }
        }
        if !current.is_empty() {
            words.push((start, current));
        }
    }

    let passage_for_handler = passage.clone();
    // A skipped letter must not leave the user stuck one letter short of the end.
    let letters_to_spare = (passage_len as f32 * (1.0 - min_accuracy)) as usize;
    let accuracy_text =
        t(&translations, "challenge.typing.accuracy").replace("{pct}", &accuracy_pct.to_string());
    let min_accuracy_text = t(&translations, "challenge.typing.min_accuracy")
        .replace("{pct}", &min_accuracy_pct.to_string());
    let progress_text = t(&translations, "challenge.typing.progress")
        .replace("{typed}", &typed_len.to_string())
        .replace("{total}", &passage_len.to_string());

    rsx! {
        div { class: "challenge-wrapper",
            p { class: "challenge-instruction", "{instruction}" }

            // Passage display: each word is its own inline-block, never breaking mid-word.
            div { class: "typing-passage",
                for (word_idx, (word_start, word)) in words.iter().enumerate() {
                    {
                        let word_chars: Vec<char> = word.chars().collect();
                        let start = *word_start;
                        rsx! {
                            span {
                                class: "typing-word",
                                key: "word-{word_idx}",
                                for (ci, ch) in word_chars.iter().enumerate() {
                                    {
                                        let absolute_idx = start + ci;
                                        let class_name = if absolute_idx < typed_len {
                                            let typed_lc = typed_chars[absolute_idx].to_ascii_lowercase();
                                            let expected_lc = ch.to_ascii_lowercase();
                                            if typed_lc == expected_lc { "correct" } else { "incorrect" }
                                        } else if absolute_idx == typed_len {
                                            "cursor pending"
                                        } else {
                                            "pending"
                                        };
                                        rsx! {
                                            span { class: "{class_name}", "{ch}" }
                                        }
                                    }
                                }
                            }
                            // Render the space after each word as a normal span (so it can wrap)
                            if word_idx < words.len() - 1 {
                                {
                                    let space_idx = start + word_chars.len();
                                    let space_class = if space_idx < typed_len {
                                        if typed_chars[space_idx] == ' ' { "correct" } else { "incorrect" }
                                    } else if space_idx == typed_len {
                                        "cursor pending"
                                    } else {
                                        "pending"
                                    };
                                    rsx! {
                                        span { class: "{space_class}", "\u{00A0}" }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Input area
            textarea {
                class: "typing-input-area",
                placeholder: "{placeholder}",
                autofocus: true,
                autocomplete: "off",
                autocorrect: "off",
                autocapitalize: "off",
                spellcheck: "false",
                // No value from here: one that comes back late would overwrite what was typed
                // since, and lose a letter.
                oninput: move |evt| {
                    let val = evt.value();
                    let typed = val.chars().count();
                    let total = passage_for_handler.chars().count();
                    let accurate = crate::models::typing_accuracy(&passage_for_handler, &val)
                        >= min_accuracy;
                    typed_text.set(val);
                    // All of it typed, with too much of it wrong.
                    wrong.set(typed >= total && !accurate);
                    // Done once close enough to the end, with enough right.
                    if accurate && typed + letters_to_spare >= total {
                        on_complete.call(());
                    }
                },
            }

            // Accuracy display
            div { class: "typing-accuracy",
                span { class: "{accuracy_class}",
                    "{accuracy_text}"
                }
                span { "{min_accuracy_text}" }
            }

            if wrong() {
                p { class: "run-wrong", role: "status", "{incorrect_text}" }
            }

            // Progress
            p { class: "tap-hint",
                "{progress_text}"
            }
        }
    }
}
