//! Drag-to-reorder for lists of rows (touch friendly).
//!
//! The touch handling lives in `reorder.js`, run in the WebView: pointer
//! events, a long-press (or a drag handle), auto-scroll while dragging, and
//! swallowing the click that ends a drag. When a row is dropped elsewhere the
//! page reports `(from, to)` row positions and the list's owner reorders its
//! data. Rows move visually only while dragging; Rust stays the source of truth.

use dioxus::prelude::*;

const SCRIPT: &str = include_str!("reorder.js");

/// Make the rows (`.row_class`) inside `#container_id` draggable.
///
/// With `handle_class` the drag starts at once on that element inside a row;
/// without it, a long press on the row starts the drag. `on_move` gets the
/// row's old and new position.
pub fn use_reorder(
    container_id: &'static str,
    row_class: &'static str,
    handle_class: Option<&'static str>,
    on_move: EventHandler<(usize, usize)>,
) {
    use_future(move || async move {
        let handle = match handle_class {
            Some(class) => format!("{class:?}"),
            None => "null".to_owned(),
        };
        let script = format!(
            "({SCRIPT})({container_id:?}, {row_class:?}, {handle}, {});",
            LONG_PRESS_MS
        );
        let mut eval = document::eval(&script);
        while let Ok(moved) = eval.recv::<(usize, usize)>().await {
            on_move.call(moved);
        }
    });
}

/// How long a row has to be held before it can be dragged.
const LONG_PRESS_MS: u32 = 450;
