use dioxus::prelude::*;

use crate::app::{t, Translations};

/// Memory game phases
#[derive(Clone, PartialEq)]
enum MemoryPhase {
    /// Showing the sequence to the user
    Display,
    /// User is reproducing the sequence
    Input,
}

/// Maximum wrong attempts before resetting and re-showing the sequence.
const MAX_WRONG_ATTEMPTS: usize = 3;

/// Memory game challenge: sequence display phase, then input phase on a grid.
/// - Display phase: cells light up one at a time (1 second per cell)
/// - Input phase: user taps cells to reproduce the sequence
/// - 3x3 grid (9 cells) for Easy/Medium, 4x3 (12 cells) for Hard/Extreme
/// - Up to 3 wrong attempts allowed; after 3 wrongs, sequence is re-shown.
/// - Wrong cells turn red so the user can see what they've tried.
#[component]
pub fn MemoryGameChallenge(
    sequence: Vec<u8>,
    grid_size: u8,
    on_complete: EventHandler<Vec<u8>>,
) -> Element {
    let translations = use_context::<Translations>();
    let instruction_watch = t(&translations, "challenge.instruction.memory_watch");
    let instruction_input = t(&translations, "challenge.instruction.memory_input");
    let hint_display = t(&translations, "challenge.memory.hint_display");
    let hint_input = t(&translations, "challenge.memory.hint_input");
    let mut phase = use_signal(|| MemoryPhase::Display);
    let mut display_index = use_signal(|| 0usize);
    let mut user_sequence: Signal<Vec<u8>> = use_signal(Vec::new);
    let mut wrong_cells: Signal<Vec<u8>> = use_signal(Vec::new);

    let seq_len = sequence.len();
    let total_cells = if grid_size >= 4 { 12u8 } else { 9u8 };
    let cols = if grid_size >= 4 { 4 } else { 3 };

    // Display phase task — call `.restart()` to replay the sequence.
    let sequence_for_display = sequence.clone();
    let mut display_task = use_future(move || {
        let seq = sequence_for_display.clone();
        async move {
            phase.set(MemoryPhase::Display);
            user_sequence.write().clear();
            wrong_cells.write().clear();
            display_index.set(0);

            // Wait briefly before starting (gives prior animation time to settle)
            sleep(300).await;

            for i in 0..seq.len() {
                display_index.set(i);
                sleep(1000).await;
            }
            // Brief pause after showing all cells
            display_index.set(seq.len()); // past end = nothing highlighted
            sleep(500).await;
            phase.set(MemoryPhase::Input);
        }
    });

    let grid_class = if cols == 4 {
        "memory-grid memory-grid-4"
    } else {
        "memory-grid memory-grid-3"
    };

    let current_display = *display_index.read();
    let current_phase = phase.read().clone();
    let user_seq_len = user_sequence.read().len();
    let wrong_count = wrong_cells.read().len();
    let instruction_text = match &current_phase {
        MemoryPhase::Display => instruction_watch.clone(),
        MemoryPhase::Input => instruction_input.clone(),
    };
    let progress_text = t(&translations, "challenge.memory.progress")
        .replace("{done}", &user_seq_len.to_string())
        .replace("{total}", &seq_len.to_string());
    let wrong_text = t(&translations, "challenge.memory.wrong_count")
        .replace("{wrong}", &wrong_count.to_string())
        .replace("{max}", &MAX_WRONG_ATTEMPTS.to_string());

    rsx! {
        div { class: "challenge-wrapper",
            p { class: "challenge-instruction", "{instruction_text}" }

            // Status line is always rendered (fixed height) so the grid never
            // shifts when the progress/wrong counters appear during input.
            p { class: "challenge-progress challenge-progress-fixed",
                if current_phase == MemoryPhase::Input {
                    "{progress_text}"
                    if wrong_count > 0 {
                        "{wrong_text}"
                    }
                }
            }

            div { class: "{grid_class}",
                for cell_idx in 0..total_cells {
                    {
                        let is_active = current_phase == MemoryPhase::Display
                            && current_display < seq_len
                            && sequence[current_display] == cell_idx;

                        let is_selected = current_phase == MemoryPhase::Input
                            && user_sequence.read().contains(&cell_idx);

                        let is_wrong = wrong_cells.read().contains(&cell_idx);

                        let cell_class = if is_wrong {
                            "memory-cell wrong"
                        } else if is_active {
                            "memory-cell active"
                        } else if is_selected {
                            "memory-cell selected"
                        } else {
                            "memory-cell"
                        };

                        let sequence_for_click = sequence.clone();

                        rsx! {
                            div {
                                class: "{cell_class}",
                                onclick: move |_| {
                                    if *phase.read() != MemoryPhase::Input {
                                        return;
                                    }
                                    // Don't allow clicking already-marked-wrong cells
                                    if wrong_cells.read().contains(&cell_idx) {
                                        return;
                                    }
                                    // Don't allow re-clicking already-selected cells
                                    if user_sequence.read().contains(&cell_idx) {
                                        return;
                                    }

                                    let next_expected_idx = user_sequence.read().len();
                                    let is_correct = next_expected_idx < sequence_for_click.len()
                                        && sequence_for_click[next_expected_idx] == cell_idx;

                                    if is_correct {
                                        user_sequence.write().push(cell_idx);
                                        // A correct press clears any wrong marks: the red
                                        // tiles reset and the wrong-attempt budget is full
                                        // again. Without this, a cell marked wrong earlier
                                        // (because it was tapped out of order) stays red and
                                        // blocked even once it becomes the correct next tile.
                                        if !wrong_cells.read().is_empty() {
                                            wrong_cells.write().clear();
                                        }

                                        // Sequence complete?
                                        if user_sequence.read().len() == sequence_for_click.len() {
                                            let result = user_sequence.read().clone();
                                            on_complete.call(result);
                                        }
                                    } else {
                                        // Wrong cell — add to wrong list
                                        wrong_cells.write().push(cell_idx);

                                        // 3 wrong = reset and replay the sequence
                                        if wrong_cells.read().len() >= MAX_WRONG_ATTEMPTS {
                                            display_task.restart();
                                        }
                                    }
                                },
                                if is_selected {
                                    {
                                        let pos = user_sequence.read().iter().position(|&x| x == cell_idx).unwrap_or(0) + 1;
                                        rsx! { "{pos}" }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            if current_phase == MemoryPhase::Display {
                p { class: "tap-hint", "{hint_display}" }
            } else {
                p { class: "tap-hint", "{hint_input}" }
            }
        }
    }
}

/// Async sleep helper using oneshot channel
async fn sleep(ms: u64) {
    let (tx, rx) = futures_channel::oneshot::channel::<()>();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(ms));
        let _ = tx.send(());
    });
    let _ = rx.await;
}
