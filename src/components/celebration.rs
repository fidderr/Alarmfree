use dioxus::prelude::*;

use crate::components::{Icon, IconName};

/// Celebration overlay shown briefly when a challenge is solved.
/// Big green checkmark with a confetti burst — pure CSS animations.
/// Lifecycle is controlled by the parent (mount when active, unmount when fading).
#[component]
pub fn Celebration(
    /// Optional progress text shown below the checkmark, e.g. "2 / 3"
    #[props(default = String::new())]
    label: String,
    /// Number of confetti particles to render (more = denser).
    #[props(default = 28)]
    particles: u32,
) -> Element {
    rsx! {
        div { class: "celebration-overlay",
            // Confetti burst — particles flung outward at random angles.
            div { class: "celebration-confetti",
                for i in 0..particles {
                    {
                        // Pseudo-random spread using deterministic math on the index.
                        let angle = (i as f32) * (360.0 / particles as f32);
                        let dx = (angle.to_radians().cos() * 220.0) as i32;
                        let dy = (angle.to_radians().sin() * 220.0) as i32;
                        let delay_ms = (i % 6) * 30;
                        let hue = (i * 47) % 360;
                        let size = 6 + (i % 4) * 2;
                        let style = format!(
                            "--dx: {}px; --dy: {}px; --delay: {}ms; --hue: {}; --size: {}px;",
                            dx, dy, delay_ms, hue, size
                        );
                        rsx! {
                            span {
                                class: "confetti-piece",
                                style: "{style}",
                            }
                        }
                    }
                }
            }
            div { class: "celebration-card",
                div { class: "celebration-check",
                    Icon { name: IconName::Check, size: 80 }
                }
                if !label.is_empty() {
                    p { class: "celebration-label", "{label}" }
                }
            }
        }
    }
}
