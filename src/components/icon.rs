use dioxus::prelude::*;

/// Icon names used throughout the app.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum IconName {
    Clock,
    ChartBar,
    Gear,
    Trash,
    Check,
    Xmark,
    Plus,
    ChevronRight,
    ArrowLeft,
    Grip,
    EllipsisVertical,
    Pause,
    Sort,
    Warning,
    Pencil,
    Skip,
}

/// One outline icon set, as inline SVG. No external fonts or CDN needed.
#[component]
pub fn Icon(name: IconName, #[props(default = 16)] size: u32) -> Element {
    let path = match name {
        IconName::Clock => "M21 12a9 9 0 1 1-18 0 9 9 0 0 1 18 0ZM12 7v5l3 2",
        IconName::ChartBar => "M4 20V10M10 20V4M16 20v-7M21 20H3",
        IconName::Gear => "M4 7h10M18 7h2M4 17h2M10 17h10M18 7a2 2 0 1 1-4 0 2 2 0 0 1 4 0ZM10 17a2 2 0 1 1-4 0 2 2 0 0 1 4 0Z",
        IconName::Trash => "M4 7h16M9 7V4h6v3M6 7l1 13h10l1-13",
        IconName::Check => "M5 12.5 10 17.5 19 7",
        IconName::Xmark => "M6 6l12 12M18 6 6 18",
        IconName::Pencil => "M4 20h4L19 9l-4-4L4 16v4ZM13.5 6.5l4 4",
        IconName::Skip => "M5 5l10 7-10 7V5ZM19 5v14",
        IconName::Plus => "M12 5v14M5 12h14",
        IconName::ChevronRight => "m9 5 7 7-7 7",
        IconName::ArrowLeft => "m15 5-7 7 7 7",
        IconName::Grip => "M5 9h14M5 15h14",
        IconName::EllipsisVertical => "M12 5h.01M12 12h.01M12 19h.01",
        IconName::Pause => "M9 5v14M15 5v14",
        IconName::Sort => "M4 7h16M7 12h10M10 17h4",
        IconName::Warning => "M12 9v4M12 17h.01M10.3 3.9 2.4 18a2 2 0 0 0 1.7 3h15.8a2 2 0 0 0 1.7-3L13.7 3.9a2 2 0 0 0-3.4 0Z",
    };
    // The dots only show with a wider line.
    let weight = if name == IconName::EllipsisVertical {
        "3"
    } else {
        "2"
    };

    rsx! {
        svg {
            class: "icon",
            width: "{size}",
            height: "{size}",
            view_box: "0 0 24 24",
            fill: "none",
            stroke: "currentColor",
            stroke_width: "{weight}",
            stroke_linecap: "round",
            stroke_linejoin: "round",
            "aria-hidden": "true",
            path { d: "{path}" }
        }
    }
}
