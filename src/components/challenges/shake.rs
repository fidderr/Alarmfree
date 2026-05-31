use dioxus::prelude::*;

use crate::app::{t, Translations};

/// Shake challenge: shake the device to count shakes.
/// Uses the accelerometer via mobile-sentinel's sensor API.
/// Auto-completes when target reached.
#[component]
pub fn ShakeChallenge(required: u32, on_complete: EventHandler<()>) -> Element {
    let translations = use_context::<Translations>();
    let instruction = t(&translations, "challenge.shake.instruction");
    let unit = t(&translations, "challenge.shake.unit");
    let mut current_count = use_signal(|| 0u32);
    let mut started = use_signal(|| false);

    // Start accelerometer on first render
    if !*started.read() {
        started.set(true);
        mobile_sentinel::sensors::reset_shake_count();
        mobile_sentinel::sensors::start_accelerometer();
    }

    // Poll shake count from mobile-sentinel every 200ms
    let _poll = use_future(move || async move {
        loop {
            let (tx, rx) = futures_channel::oneshot::channel::<()>();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(200));
                let _ = tx.send(());
            });
            let _ = rx.await;

            let count = mobile_sentinel::sensors::shake_count() as u32;
            current_count.set(count);

            if count >= required {
                mobile_sentinel::sensors::stop_accelerometer();
                on_complete.call(());
                break;
            }
        }
    });

    // Stop accelerometer on unmount
    use_drop(|| {
        mobile_sentinel::sensors::stop_accelerometer();
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
