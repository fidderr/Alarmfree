use dioxus::prelude::*;

use super::ProgressRing;
use crate::app::{t, Translations};

/// Walk a number of steps, by the phone's step counter.
#[component]
pub fn StepCountChallenge(required: u32, on_complete: EventHandler<()>) -> Element {
    let translations = use_context::<Translations>();
    let instruction = t(&translations, "challenge.steps.instruction");
    let unit = t(&translations, "challenge.steps.unit");
    let mut count = use_signal(|| 0u32);

    // An effect, so after the previous challenge is gone: one that also counts steps
    // switches the sensor off when it goes.
    use_effect(mobile_sentinel::sensors::start_step_counter);
    use_drop(mobile_sentinel::sensors::stop_step_counter);

    use_future(move || async move {
        loop {
            crate::timer::sleep_ms(500).await;
            let now = mobile_sentinel::sensors::step_count() as u32;
            count.set(now);
            if now >= required {
                on_complete.call(());
                break;
            }
        }
    });

    rsx! {
        div { class: "challenge-wrapper",
            p { class: "challenge-instruction", "{instruction}" }
            ProgressRing { count: count(), required, unit }
        }
    }
}
