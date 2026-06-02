use dioxus::prelude::*;
use std::time::Duration;

use crate::app::{t, Translations};

/// Pop animation duration (ms) shown when a bubble fills. Must match the
/// `bubble-pop` / ripple animation durations in style.css.
const POP_MS: u64 = 280;

/// Hold-the-bubble challenge: a bubble appears at a random spot in the
/// play area. Press and hold it until its ring fills, then it pops and a
/// fresh bubble springs in somewhere else. Repeat `required` times.
///
/// - `required`: number of bubbles to complete.
/// - `hold_ms`: how long each bubble must be held to fill.
/// - `on_cancel`: optional — when set, a back button is shown (used when
///   this is the fallback challenge so a mis-tap can be undone).
///
/// The fill/drain is driven entirely by CSS transitions (GPU-composited,
/// perfectly smooth) — Rust only flips `filling` on press/release and runs
/// a single completion timer per hold. Press starts on the bubble; release
/// is detected on the whole area so finger drift never cancels the hold.
#[component]
pub fn HoldButtonChallenge(
    required: u32,
    hold_ms: u32,
    on_complete: EventHandler<()>,
    #[props(default)] on_cancel: Option<EventHandler<()>>,
) -> Element {
    let translations = use_context::<Translations>();
    let instruction = t(&translations, "challenge.hold.instruction");
    let unit = t(&translations, "challenge.hold.unit");
    let hold_label = t(&translations, "challenge.hold.hold");
    let back_label = t(&translations, "action.back");

    let hold_ms = hold_ms.max(300); // guard against too-fast configs
    let required = required.max(1);

    // Number of bubbles completed so far.
    let mut completed = use_signal(|| 0u32);
    // Whether the finger is currently pressing (drives the CSS fill).
    let mut filling = use_signal(|| false);
    // Pop animation flag while a filled bubble bursts.
    let mut popping = use_signal(|| false);
    // Position of the current bubble *center* as (top%, left%) in the area.
    let mut pos = use_signal(random_pos);
    // Bumped per bubble so each gets a fresh element (clean empty ring).
    let mut bubble_seq = use_signal(|| 0u32);
    // Invalidates an in-flight completion timer when the user releases.
    let mut hold_token = use_signal(|| 0u32);

    let mut start_hold = move || {
        if *popping.read() || *filling.read() {
            return;
        }
        let token = hold_token() + 1;
        hold_token.set(token);
        filling.set(true);
        haptic(18);

        // Completion timer: if the hold survives `hold_ms` uninterrupted,
        // this bubble is done. A release bumps `hold_token`, invalidating
        // this exact timer.
        spawn(async move {
            sleep_ms(hold_ms as u64).await;
            if hold_token() != token || *popping.read() {
                return;
            }
            let done = completed() + 1;
            completed.set(done);
            filling.set(false);
            popping.set(true);
            haptic(if done >= required { 130 } else { 45 });

            sleep_ms(POP_MS).await;
            if done >= required {
                on_complete.call(());
            } else {
                popping.set(false);
                pos.set(random_pos());
                bubble_seq.set(bubble_seq() + 1);
            }
        });
    };

    let mut release = move || {
        // Invalidate the pending completion and let the ring drain.
        if *filling.read() {
            hold_token.set(hold_token() + 1);
            filling.set(false);
        }
    };

    let done = *completed.read();
    let is_filling = *filling.read();
    let is_popping = *popping.read();
    let (fy, fx) = *pos.read();
    let _seq = bubble_seq();

    let bubble_class = if is_popping {
        "hold-bubble popping"
    } else if is_filling {
        "hold-bubble filling"
    } else {
        "hold-bubble"
    };
    let inner_label = if is_popping { "\u{2713}" } else { &hold_label };
    // Position the slot centre with a fixed-px safe margin (--hold-safe in
    // the CSS) so the bubble + its glow can never reach the clipped edge of
    // the play area, regardless of the area's pixel size. `--fx`/`--fy` are
    // 0..1 fractions across the safe band.
    let slot_style = format!("--fx: {fx}; --fy: {fy}; --hold-ms: {hold_ms}ms;");

    rsx! {
        div { class: "challenge-wrapper hold-wrapper",
            if let Some(cancel) = on_cancel {
                button {
                    class: "hold-back-btn",
                    "aria-label": "{back_label}",
                    onpointerdown: move |evt| { evt.stop_propagation(); },
                    onclick: move |_| cancel.call(()),
                    "\u{2039}"
                }
            }
            div { class: "hold-header",
                p { class: "challenge-instruction hold-instruction", "{instruction}" }
            }

            // Release is handled at the area level so small drift off the
            // bubble (but still inside the area) never cancels the hold.
            div {
                class: "hold-area",
                onpointerup: move |_| release(),
                onpointercancel: move |_| release(),
                onpointerleave: move |_| release(),

                div {
                    key: "bubble-{_seq}",
                    class: "hold-bubble-slot",
                    style: "{slot_style}",

                    div {
                        class: "{bubble_class}",
                        onpointerdown: move |_| start_hold(),
                        oncontextmenu: move |evt| evt.prevent_default(),

                        svg { class: "hold-ring", view_box: "0 0 100 100",
                            circle { class: "hold-ring-track", cx: "50", cy: "50", r: "45" }
                            circle { class: "hold-ring-fill", cx: "50", cy: "50", r: "45" }
                        }
                        div { class: "hold-bubble-core",
                            span { class: "hold-bubble-label", "{inner_label}" }
                        }
                    }
                }
            }

            p { class: "progress-ring-label hold-progress", "{done} / {required} {unit}" }
        }
    }
}

/// Random bubble position as 0..1 fractions across the safe band on each
/// axis. The CSS maps these into the area with a fixed-px safe margin so
/// the bubble + glow never touch the clipped edge.
fn random_pos() -> (f32, f32) {
    (rand::random::<f32>(), rand::random::<f32>())
}

/// Fire a short haptic pulse via the gated haptics capability. No-op if
/// unavailable. (Reaching vibration through the raw sink is no longer
/// possible — `haptics` must be declared as a feature.)
fn haptic(_ms: u64) {
    #[cfg(target_os = "android")]
    mobile_sentinel::haptics::vibrate(Duration::from_millis(_ms));
}

/// Async sleep via a background thread + oneshot — keeps the UI layer
/// free of a tokio dependency (same pattern the other challenges use).
async fn sleep_ms(ms: u64) {
    let (tx, rx) = futures_channel::oneshot::channel::<()>();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(ms));
        let _ = tx.send(());
    });
    let _ = rx.await;
}
