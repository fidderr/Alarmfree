use dioxus::prelude::*;

use crate::app::{t, Translations};

/// Step count challenge: walk the required number of steps.
/// Uses the device's step counter sensor via mobile-sentinel's sensor API.
/// Auto-completes when target reached.
#[component]
pub fn StepCountChallenge(required: u32, on_complete: EventHandler<()>) -> Element {
    let translations = use_context::<Translations>();
    let instruction = t(&translations, "challenge.steps.instruction");
    let unit = t(&translations, "challenge.steps.unit");
    let mut current_count = use_signal(|| 0u32);
    let mut started = use_signal(|| false);

    // Start step counter on first render
    if !*started.read() {
        started.set(true);
        mobile_sentinel::sensors::start_step_counter();
    }

    // Poll step count from mobile-sentinel every 500ms
    let _poll = use_future(move || async move {
        loop {
            let (tx, rx) = futures_channel::oneshot::channel::<()>();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(500));
                let _ = tx.send(());
            });
            let _ = rx.await;

            let count = mobile_sentinel::sensors::step_count() as u32;
            current_count.set(count);

            if count >= required {
                mobile_sentinel::sensors::stop_step_counter();
                on_complete.call(());
                break;
            }
        }
    });

    // Stop step counter on unmount
    use_drop(|| {
        mobile_sentinel::sensors::stop_step_counter();
    });

    let count = *current_count.read();
    let progress = if required > 0 {
        ((count as f64 / required as f64) * 360.0) as u32
    } else {
        360
    };
    let rotation = progress.min(360);

    rsx! {
        div { class: "challenge-wrapper",
            p { class: "challenge-instruction", "{instruction}" }

            div { class: "progress-ring-container",
                div {
                    class: "progress-ring",
                    div {
                        class: "progress-ring-fill",
                        style: "transform: rotate({rotation}deg);",
                    }
                    div { class: "progress-ring-text", "{count} / {required}" }
                }
                p { class: "progress-ring-label", "{unit}" }
            }
        }
    }
}
