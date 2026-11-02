//! The building blocks every screen is made of, so they look and behave alike.

use dioxus::prelude::*;

use crate::app::{t, Route, Translations};
use crate::back::use_back_handler;
use crate::components::{Icon, IconName};

/// A panel over the screen, from the bottom. Back and tapping beside it close it.
#[component]
pub fn Sheet(
    title: String,
    #[props(default)] note: String,
    on_close: EventHandler<()>,
    children: Element,
) -> Element {
    use_back_handler(on_close);
    rsx! {
        button {
            class: "sheet-scrim",
            "aria-label": "{title}",
            onclick: move |_| on_close.call(()),
        }
        div { class: "sheet", role: "dialog", "aria-label": "{title}",
            span { class: "sheet-grab", "aria-hidden": "true" }
            h2 { class: "sheet-title", "{title}" }
            if !note.is_empty() {
                p { class: "sheet-note", "{note}" }
            }
            {children}
        }
    }
}

/// The main button at the bottom of a sheet.
#[component]
pub fn SheetDone(label: String, on_click: EventHandler<()>) -> Element {
    rsx! {
        button { class: "btn btn-primary sheet-done", onclick: move |_| on_click.call(()), "{label}" }
    }
}

/// A plain choice in a sheet's list.
#[component]
pub fn SheetItem(
    label: String,
    on_click: EventHandler<()>,
    #[props(default)] danger: bool,
    #[props(default)] selected: bool,
) -> Element {
    let class = match (danger, selected) {
        (true, _) => "sheet-item danger",
        (_, true) => "sheet-item selected",
        _ => "sheet-item",
    };
    rsx! {
        button {
            class: "{class}",
            "aria-pressed": selected.then_some("true"),
            onclick: move |_| on_click.call(()),
            "{label}"
        }
    }
}

/// A heading above a group of controls in a sheet.
#[component]
pub fn GroupTitle(text: String) -> Element {
    rsx! {
        p { class: "group-title", "{text}" }
    }
}

/// Pick one of a few options.
#[component]
pub fn Chips<T: Clone + PartialEq + 'static>(
    options: Vec<(T, String)>,
    selected: Option<T>,
    on_pick: EventHandler<T>,
    /// Stretch to fill the width, for options of about the same length.
    #[props(default)]
    grow: bool,
) -> Element {
    let row_class = if grow { "chips chips-grow" } else { "chips" };
    rsx! {
        div { class: "{row_class}",
            for (value, label) in options {
                {
                    let is_selected = selected.as_ref() == Some(&value);
                    rsx! {
                        button {
                            class: if is_selected { "chip selected" } else { "chip" },
                            "aria-pressed": if is_selected { "true" } else { "false" },
                            onclick: move |_| on_pick.call(value.clone()),
                            "{label}"
                        }
                    }
                }
            }
        }
    }
}

/// A number with less and more buttons.
#[component]
pub fn Stepper(
    title: String,
    value: String,
    on_less: EventHandler<()>,
    on_more: EventHandler<()>,
) -> Element {
    let translations = use_context::<Translations>();
    let less = format!("{} {title}", t(&translations, "action.decrease"));
    let more = format!("{} {title}", t(&translations, "action.increase"));
    rsx! {
        div { class: "stepper",
            span { class: "stepper-title", "{title}" }
            span { class: "stepper-controls",
                button { class: "round-btn", "aria-label": "{less}", onclick: move |_| on_less.call(()), "\u{2212}" }
                span { class: "stepper-value", "{value}" }
                button { class: "round-btn", "aria-label": "{more}", onclick: move |_| on_more.call(()), "+" }
            }
        }
    }
}

/// One line of a list: what it is, what it is set to, and it opens something.
#[component]
pub fn Row(title: String, #[props(default)] value: String, on_click: EventHandler<()>) -> Element {
    rsx! {
        li { class: "row",
            button { class: "row-btn", onclick: move |_| on_click.call(()),
                span { class: "row-title", "{title}" }
                span { class: "row-value", "{value}" }
                Icon { name: IconName::ChevronRight, size: 14 }
            }
        }
    }
}

/// One line of a list with a switch.
#[component]
pub fn SwitchRow(title: String, on: bool, on_toggle: EventHandler<bool>) -> Element {
    rsx! {
        li { class: "row",
            button {
                class: "row-btn",
                role: "switch",
                "aria-checked": if on { "true" } else { "false" },
                onclick: move |_| on_toggle.call(!on),
                span { class: "row-title", "{title}" }
                span { class: "row-value" }
                Switch { on }
            }
        }
    }
}

/// The look of a switch; the button around it does the work.
#[component]
pub fn Switch(on: bool) -> Element {
    rsx! {
        span { class: if on { "switch on" } else { "switch" }, "aria-hidden": "true",
            span { class: "switch-knob" }
        }
    }
}

/// The top of a screen below the main three.
#[component]
pub fn ScreenHeader(
    title: String,
    on_back: EventHandler<()>,
    #[props(default)] children: Element,
) -> Element {
    let translations = use_context::<Translations>();
    let back = t(&translations, "action.back");
    rsx! {
        header { class: "screen-header",
            button { class: "icon-btn", "aria-label": "{back}", onclick: move |_| on_back.call(()),
                Icon { name: IconName::ArrowLeft, size: 20 }
            }
            h1 { class: "screen-title", "{title}" }
            {children}
        }
    }
}

/// The three main screens. Moving between them does not stack up: Back from any of
/// them is one step, to Alarms.
#[component]
pub fn BottomNav(current: Route) -> Element {
    let translations = use_context::<Translations>();
    let nav = use_navigator();
    let items = [
        (Route::Home {}, IconName::Clock, "nav.alarms"),
        (Route::History {}, IconName::ChartBar, "nav.history"),
        (Route::Settings {}, IconName::Gear, "nav.settings"),
    ];
    rsx! {
        nav { class: "bottom-nav",
            for (route, icon, key) in items {
                {
                    let active = route == current;
                    let label = t(&translations, key);
                    rsx! {
                        button {
                            class: if active { "nav-item active" } else { "nav-item" },
                            "aria-current": active.then_some("page"),
                            onclick: move |_| {
                                if !active {
                                    nav.replace(route.clone());
                                }
                            },
                            Icon { name: icon, size: 22 }
                            span { "{label}" }
                        }
                    }
                }
            }
        }
    }
}
