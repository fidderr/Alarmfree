use dioxus::prelude::*;
use std::time::Duration;

use crate::app::{t, Translations};

/// Number of targets visible on screen at once. Popping one instantly spawns
/// a replacement (until the round's quota is exhausted), so the player can go
/// as fast as they can tap without waiting. Kept small so targets never crowd
/// the play area on small screens.
const VISIBLE_TARGETS: usize = 3;

/// Lead-in before the very first round (ms) — a "get ready" countdown.
const FIRST_COUNTDOWN_MS: u64 = 3000;
/// Shorter lead-in between subsequent rounds (ms).
const ROUND_LEAD_MS: u64 = 1500;
/// How often the game loop polls for round-clear / time-out (ms).
const POLL_MS: u64 = 40;

/// Minimum distance (in 0..1 play-area fraction units) between target centres,
/// so freshly-spawned targets never stack on or crowd an existing one.
const MIN_SEPARATION: f32 = 0.34;

/// Pick a random position (0..1 fractions) that stays at least
/// [`MIN_SEPARATION`] from every existing target. Uses rejection sampling
/// with a bounded number of attempts; if no attempt clears the threshold
/// (a very crowded board), it returns the candidate that maximises the
/// distance to the nearest target — so targets are always as spread out as
/// possible and never exactly stacked.
fn pick_position(existing: &[Target]) -> (f32, f32) {
    let mut best = (rand::random::<f32>(), rand::random::<f32>());
    let mut best_min_dist = f32::NEG_INFINITY;
    for _ in 0..32 {
        let cand = (rand::random::<f32>(), rand::random::<f32>());
        let min_dist = existing
            .iter()
            .map(|t| {
                let dx = t.fx - cand.0;
                let dy = t.fy - cand.1;
                (dx * dx + dy * dy).sqrt()
            })
            .fold(f32::INFINITY, f32::min);
        if existing.is_empty() || min_dist >= MIN_SEPARATION {
            return cand;
        }
        if min_dist > best_min_dist {
            best_min_dist = min_dist;
            best = cand;
        }
    }
    best
}

/// One on-screen target: a stable slot id (for keying/animation) plus its
/// random position as 0..1 fractions across the play-area safe band.
#[derive(Clone, Copy, PartialEq)]
struct Target {
    id: u32,
    fx: f32,
    fy: f32,
}

/// Reaction challenge: pop `targets_per_round` targets within `time_limit`,
/// for `rounds` rounds. Only [`VISIBLE_TARGETS`] show at a time; popping one
/// instantly spawns a fresh target until the round quota is met.
///
/// - Round 1 opens with a 3s "get ready" countdown; later rounds use a shorter
///   lead-in.
/// - Miss the time limit with targets remaining → reset to round 1 (full
///   countdown again).
/// - Clear every round → solved.
///
/// A single long-lived game-loop future owns round progression (countdown →
/// reveal → clear/timeout → next round / reset). The tap handler only pops a
/// target, bumps the round counter, and tops the board back up — so there is
/// no self-referential round function. Reuses the hold challenge's full-screen
/// play-area design; only the targets look different.
#[component]
pub fn ReactionChallenge(
    targets_per_round: u32,
    time_limit_ms: u32,
    rounds: u32,
    on_complete: EventHandler<()>,
    #[props(default)] on_cancel: Option<EventHandler<()>>,
) -> Element {
    let translations = use_context::<Translations>();
    let instruction = t(&translations, "challenge.reaction.instruction");
    let get_ready = t(&translations, "challenge.reaction.get_ready");
    let back_label = t(&translations, "action.back");

    let targets_per_round = targets_per_round.max(1);
    let rounds = rounds.max(1);
    let time_limit_ms = (time_limit_ms.max(1000)) as u64;

    // True while showing the pre-round "get ready" countdown.
    let mut counting_down = use_signal(|| true);
    let mut countdown_secs = use_signal(|| (FIRST_COUNTDOWN_MS / 1000) as u32);
    // Current round (1-based) and targets popped *this* round.
    let mut round = use_signal(|| 1u32);
    let mut popped_this_round = use_signal(|| 0u32);
    // Fixed-length board of slots. Each slot keeps a stable position in the
    // DOM (keyed by slot index) so popping one NEVER reorders the others —
    // reordering a DOM node restarts its CSS animation, which is what made
    // every surviving target replay the spawn animation. A slot holds
    // `Some(target)` when occupied, `None` when empty; only the inner target
    // element (keyed by its id) re-mounts on change, so just the freshly
    // spawned target animates in.
    let slot_count = (VISIBLE_TARGETS as u32).min(targets_per_round) as usize;
    let mut slots: Signal<Vec<Option<Target>>> = use_signal(|| vec![None; slot_count]);
    // Monotonic id source for fresh targets (stable keys + spawn animation).
    let mut next_id = use_signal(|| 0u32);
    // Whether the round timer bar is actively draining (drives the CSS).
    let mut timer_on = use_signal(|| false);
    // Bumped whenever the timer bar should (re)start, so its element re-keys.
    let mut timer_epoch = use_signal(|| 0u32);
    // Set true when the challenge is fully solved, so the loop can exit.
    let mut solved = use_signal(|| false);

    // Currently-occupied targets (for collision-avoidance positioning).
    let occupied = move || -> Vec<Target> { slots.read().iter().flatten().copied().collect() };

    // The game loop. One future, runs the whole state machine.
    use_future(move || async move {
        let mut current_round = 1u32;
        let mut first = true;
        loop {
            // ---- Lead-in countdown ----
            round.set(current_round);
            popped_this_round.set(0);
            for s in slots.write().iter_mut() {
                *s = None;
            }
            timer_on.set(false);
            counting_down.set(true);

            let lead = if first {
                FIRST_COUNTDOWN_MS
            } else {
                ROUND_LEAD_MS
            };
            first = false;
            let mut remaining = lead;
            countdown_secs.set(remaining.div_ceil(1000) as u32);
            while remaining > 0 {
                let step = remaining.min(1000);
                sleep_ms(step).await;
                remaining -= step;
                countdown_secs.set(remaining.div_ceil(1000) as u32);
            }

            // ---- Reveal targets, start the round ----
            counting_down.set(false);
            for i in 0..slot_count {
                let id = next_id();
                next_id.set(id + 1);
                let (fx, fy) = pick_position(&occupied());
                slots.write()[i] = Some(Target { id, fx, fy });
            }
            timer_epoch.set(timer_epoch() + 1);
            timer_on.set(true);

            // ---- Play: poll for round-clear or time-out ----
            let mut elapsed = 0u64;
            let mut failed = false;
            loop {
                if popped_this_round() >= targets_per_round {
                    break; // round cleared
                }
                if elapsed >= time_limit_ms {
                    failed = true;
                    break;
                }
                sleep_ms(POLL_MS).await;
                elapsed += POLL_MS;
            }

            timer_on.set(false);
            for s in slots.write().iter_mut() {
                *s = None;
            }

            if failed {
                // Missed the time limit — back to round 1, full countdown.
                haptic(60);
                current_round = 1;
                continue;
            }

            // Round cleared.
            if current_round >= rounds {
                haptic(130);
                solved.set(true);
                break;
            }
            haptic(80);
            current_round += 1;
        }
    });

    // Handle a target tap: clear its slot, advance the round quota, and either
    // refill the slot with a fresh target or leave it empty when few remain.
    let mut hit = move |id: u32| {
        if *counting_down.read() || *solved.read() {
            return;
        }
        // Find the slot holding this target.
        let slot_idx = slots
            .read()
            .iter()
            .position(|s| s.map(|t| t.id) == Some(id));
        let Some(idx) = slot_idx else {
            return;
        };
        haptic(15);

        let done = popped_this_round() + 1;
        popped_this_round.set(done);

        // How many targets must still be visible after this pop: capped by the
        // board size and by how many remain to be popped this round.
        let remaining_to_pop = targets_per_round.saturating_sub(done) as usize;
        let desired_visible = slot_count.min(remaining_to_pop);
        let visible_others = slots.read().iter().filter(|s| s.is_some()).count() - 1;

        if visible_others < desired_visible {
            // Refill this slot with a fresh target at a non-overlapping spot.
            // Clear it first so collision-avoidance doesn't avoid the slot we
            // are about to reuse.
            slots.write()[idx] = None;
            let new_id = next_id();
            next_id.set(new_id + 1);
            let (fx, fy) = pick_position(&occupied());
            slots.write()[idx] = Some(Target { id: new_id, fx, fy });
        } else {
            slots.write()[idx] = None;
        }
    };

    // Fire on_complete exactly once when solved.
    let mut completed_fired = use_signal(|| false);
    if *solved.read() && !*completed_fired.read() {
        completed_fired.set(true);
        on_complete.call(());
    }

    let is_counting = *counting_down.read();
    let secs = *countdown_secs.read();
    let cur_round = round();
    let done_this = popped_this_round();
    let timer_running = *timer_on.read();
    let _epoch = timer_epoch();
    let current_slots = slots.read().clone();

    // Drive the timer bar via CSS: when running, transition width to 0 over the
    // time limit. Keyed by epoch so each round restarts the transition cleanly.
    let (bar_class, bar_style) = if timer_running {
        (
            "reaction-timer-fill running",
            format!("--react-ms: {time_limit_ms}ms; width: 0%;"),
        )
    } else {
        ("reaction-timer-fill", "width: 100%;".to_string())
    };

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

            // Timer bar — shrinks over the round time limit. Keyed per epoch so
            // the CSS transition restarts each round.
            div { class: "reaction-timer",
                div {
                    key: "bar-{_epoch}",
                    class: "{bar_class}",
                    style: "{bar_style}",
                }
            }

            div { class: "hold-area reaction-area",
                if is_counting {
                    div { class: "reaction-countdown",
                        div { class: "reaction-countdown-num", "{secs}" }
                        p { class: "reaction-countdown-label", "{get_ready}" }
                    }
                } else {
                    for (slot_idx , slot) in current_slots.iter().enumerate() {
                        // Slot wrapper keyed by INDEX → never reorders, so
                        // sibling targets are never re-inserted (no animation
                        // restart). The inner target is keyed by its own id, so
                        // only a freshly-spawned target mounts + animates in.
                        div {
                            key: "slot-{slot_idx}",
                            if let Some(target) = slot {
                                {
                                    let id = target.id;
                                    let slot_style = format!("--fx: {}; --fy: {};", target.fx, target.fy);
                                    rsx! {
                                        div {
                                            key: "target-{id}",
                                            class: "reaction-target-slot",
                                            style: "{slot_style}",
                                            div {
                                                class: "reaction-target",
                                                onpointerdown: move |evt| {
                                                    evt.stop_propagation();
                                                    hit(id);
                                                },
                                                oncontextmenu: move |evt| evt.prevent_default(),
                                                span { class: "reaction-target-ring" }
                                                span { class: "reaction-target-core" }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            p { class: "progress-ring-label hold-progress",
                "{cur_round} / {rounds} \u{00B7} {done_this} / {targets_per_round}"
            }
        }
    }
}

/// Fire a short haptic pulse via the gated haptics capability. No-op if
/// unavailable.
fn haptic(_ms: u64) {
    #[cfg(target_os = "android")]
    mobile_sentinel::haptics::vibrate(Duration::from_millis(_ms));
}

/// Async sleep via a background thread + oneshot — keeps the UI layer free of
/// a tokio dependency (same pattern the other challenges use).
async fn sleep_ms(ms: u64) {
    let (tx, rx) = futures_channel::oneshot::channel::<()>();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(ms));
        let _ = tx.send(());
    });
    let _ = rx.await;
}
