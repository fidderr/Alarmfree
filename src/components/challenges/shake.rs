use dioxus::prelude::*;

use super::ProgressRing;
use crate::app::{t, Translations};

/// Shake the phone a number of times.
#[component]
pub fn ShakeChallenge(required: u32, on_complete: EventHandler<()>) -> Element {
    let translations = use_context::<Translations>();
    let instruction = t(&translations, "challenge.shake.instruction");
    let unit = t(&translations, "challenge.shake.unit");
    let mut count = use_signal(|| 0u32);

    // An effect, so after the previous challenge is gone: one that also counts shakes
    // switches the sensor off when it goes.
    use_effect(|| {
        mobile_sentinel::sensors::reset_shake_count();
        mobile_sentinel::sensors::start_accelerometer();
    });
    use_drop(mobile_sentinel::sensors::stop_accelerometer);

    use_future(move || async move {
        loop {
            crate::timer::sleep_ms(200).await;
            let now = mobile_sentinel::sensors::shake_count() as u32;
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
