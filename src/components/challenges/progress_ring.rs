use dioxus::prelude::*;

/// The length of the circle: radius 45 in a view of 100. In style.css as well.
const RING_LENGTH: f64 = 282.74;

/// How far along a challenge that counts something is.
#[component]
pub fn ProgressRing(count: u32, required: u32, unit: String) -> Element {
    let done = (f64::from(count) / f64::from(required.max(1))).min(1.0);
    let left = RING_LENGTH * (1.0 - done);
    rsx! {
        div { class: "progress-ring-container",
            div { class: "progress-ring",
                svg { class: "progress-svg", view_box: "0 0 100 100",
                    circle { class: "progress-track", cx: "50", cy: "50", r: "45" }
                    circle {
                        class: "progress-fill",
                        cx: "50",
                        cy: "50",
                        r: "45",
                        style: "stroke-dashoffset: {left}",
                    }
                }
                div { class: "progress-ring-text", "{count} / {required}" }
            }
            p { class: "progress-ring-label", "{unit}" }
        }
    }
}
