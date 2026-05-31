use dioxus::prelude::*;
#[cfg(target_os = "android")]
use std::time::Duration;

/// Vertical pixels per one step on the wheel. Equal to the rendered row
/// height (see `.wheel-cell` in style.css) so the wheel tracks the finger
/// 1:1 — dragging one row's height advances exactly one value.
const ROW_PITCH: f64 = 40.0;

/// Time picker rendered as two spinner wheels (hours / minutes).
///
/// Each wheel shows the selected value enlarged in the middle with
/// neighbours curving away above and below like a physical drum. There are
/// two ways to change a value:
///
/// - **Drag** the column to spin it. The wheel glides continuously with your
///   finger and snaps to the nearest value on release. Drag **up** to
///   increase, **down** to decrease (direct manipulation — the numbers move
///   the same direction as your finger, like iOS).
/// - **Tap a neighbour**: the row just below the centre is `+1`, two below is
///   `+2` (and the rows above are `-1` / `-2`), so a user can step quickly
///   without precise dragging.
///
/// Hours wrap 0–23, minutes wrap 0–59.
#[component]
pub fn TimePicker(hour: u8, minute: u8, on_change: EventHandler<(u8, u8)>) -> Element {
    rsx! {
        div { class: "time-picker",
            WheelColumn {
                value: hour as i32,
                modulus: 24,
                on_change: move |h: i32| on_change.call((h as u8, minute)),
            }
            span { class: "time-separator", ":" }
            WheelColumn {
                value: minute as i32,
                modulus: 60,
                on_change: move |m: i32| on_change.call((hour, m as u8)),
            }
        }
    }
}

/// Euclidean wrap into `0..modulus` (handles negatives).
fn wrap(value: i32, modulus: i32) -> i32 {
    ((value % modulus) + modulus) % modulus
}

/// A single spinner wheel for one numeric field.
#[component]
fn WheelColumn(value: i32, modulus: i32, on_change: EventHandler<i32>) -> Element {
    // Drag gesture state (persisted across the re-renders a drag triggers).
    let mut dragging = use_signal(|| false);
    let mut start_y = use_signal(|| 0.0f64);
    let mut start_value = use_signal(|| 0i32);
    let mut last_steps = use_signal(|| 0i32);
    // Live sub-step translation (px) applied to the spinning stack so it
    // glides with the finger between value snaps. Reset to 0 on release,
    // which animates the remainder back to centre (the snap).
    let mut drag_px = use_signal(|| 0.0f64);
    // Set true once a gesture actually moved, so the trailing click that a
    // touch emits after a drag doesn't apply an extra step.
    let mut moved = use_signal(|| false);

    // Rows top -> bottom: LOW values on top, HIGH on the bottom (iOS order).
    // Dragging up scrolls higher values up into the centre — the numbers
    // follow the finger (direct manipulation). Seven rows give an overflow
    // buffer so no gap shows while the stack glides.
    let offsets = [-3i32, -2, -1, 0, 1, 2, 3];

    // The stack transform follows the finger during a drag, then transitions
    // back to centre (snap) once released.
    let stack_style = if *dragging.read() {
        format!(
            "transform: translateY({}px); transition: none;",
            *drag_px.read()
        )
    } else {
        "transform: translateY(0px); transition: transform 0.24s cubic-bezier(0.22, 1, 0.36, 1);"
            .to_string()
    };

    rsx! {
        div {
            class: "wheel-column",
            onpointerdown: move |evt| {
                dragging.set(true);
                moved.set(false);
                last_steps.set(0);
                drag_px.set(0.0);
                start_value.set(value);
                start_y.set(evt.data().client_coordinates().y);
            },
            onpointermove: move |evt| {
                if !*dragging.read() {
                    return;
                }
                // dy > 0 when the finger moves down.
                let dy = evt.data().client_coordinates().y - *start_y.read();
                // Drag up (negative dy) increases the value. Round so the
                // value flips at the half-row crossing.
                let steps = (-dy / ROW_PITCH).round() as i32;
                // Keep the stack gliding with the finger: the whole-step part
                // is applied by re-rendering shifted rows (value change), so
                // only the leftover sub-step pixels translate the stack.
                drag_px.set(dy + steps as f64 * ROW_PITCH);
                if steps != *last_steps.read() {
                    if steps != 0 {
                        moved.set(true);
                    }
                    last_steps.set(steps);
                    let new_val = wrap(*start_value.read() + steps, modulus);
                    if new_val != value {
                        haptic();
                        on_change.call(new_val);
                    }
                }
            },
            onpointerup: move |_| {
                dragging.set(false);
                drag_px.set(0.0);
            },
            onpointercancel: move |_| {
                dragging.set(false);
                drag_px.set(0.0);
            },
            onpointerleave: move |_| {
                if *dragging.read() {
                    dragging.set(false);
                    drag_px.set(0.0);
                }
            },
            oncontextmenu: move |evt| evt.prevent_default(),

            div {
                class: "wheel-stack",
                style: "{stack_style}",

                for off in offsets {
                    {
                        let display = wrap(value + off, modulus);
                        let cls = match off.abs() {
                            0 => "wheel-cell wheel-cell-center",
                            1 => "wheel-cell wheel-cell-near",
                            2 => "wheel-cell wheel-cell-far",
                            _ => "wheel-cell wheel-cell-edge",
                        };
                        // Curve each row away from the centre like a drum:
                        // rows above tilt their top back, rows below tilt
                        // their bottom back. translateZ pulls them toward the
                        // axis so the column reads as a cylinder, not a fan.
                        let rot = -(off as f64) * 20.0;
                        let cell_style =
                            format!("transform: rotateX({rot}deg) translateZ(8px);");
                        rsx! {
                            div {
                                key: "{off}",
                                class: "{cls}",
                                style: "{cell_style}",
                                onclick: move |_| {
                                    // Swallow the click that trails a drag.
                                    if *moved.read() {
                                        moved.set(false);
                                        return;
                                    }
                                    if off != 0 {
                                        on_change.call(wrap(value + off, modulus));
                                    }
                                },
                                "{display:02}"
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Fire a short haptic tick via the gated haptics capability (no-op off
/// Android / when no vibrator). Mirrors the pattern used by the challenges.
fn haptic() {
    #[cfg(target_os = "android")]
    mobile_sentinel::haptics::vibrate(Duration::from_millis(8));
}
