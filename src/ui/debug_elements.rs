//! The part of the devtest channel that looks at the Slint elements: which texts are on screen
//! and where, the text fields, and a tap or drag at a place. Only with the cargo feature
//! `devtest` (a debug build made by `./build.sh`), never in a build for users: it needs the
//! compiler's debug info and the testing crate.

use i_slint_backend_testing::{ElementHandle, ElementQuery};
use slint::platform::{Key, PointerEventButton, WindowEvent};
use slint::{ComponentHandle, LogicalPosition};

use super::MainWindow;

/// One element on screen, in device pixels: its middle and its size.
struct Seen {
    text: String,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    /// What it is to a screen reader (`Button`, `Switch`, ...), and whether it is checked or selected.
    role: String,
    on: Option<bool>,
}

fn scale(ui: &MainWindow) -> f32 {
    ui.window().scale_factor()
}

/// Is the middle of the element inside the window, and is it drawn at all?
fn seen_in_window(ui: &MainWindow, e: &ElementHandle, text: String) -> Option<Seen> {
    let s = scale(ui);
    let size = e.size();
    let pos = e.absolute_position();
    let win = ui.window().size();
    let seen = Seen {
        text,
        x: (pos.x + size.width / 2.0) * s,
        y: (pos.y + size.height / 2.0) * s,
        w: size.width * s,
        h: size.height * s,
        role: e
            .accessible_role()
            .map(|r| format!("{r:?}"))
            .unwrap_or_default(),
        on: e
            .accessible_checked()
            .or_else(|| e.accessible_item_selected()),
    };
    let inside =
        seen.x >= 0.0 && seen.y >= 0.0 && seen.x < win.width as f32 && seen.y < win.height as f32;
    (inside && size.width > 0.5 && size.height > 0.5 && e.computed_opacity() > 0.01).then_some(seen)
}

fn texts(ui: &MainWindow) -> Vec<Seen> {
    // What says something to a person: a text, or a button with a label for screen readers.
    let mut found: Vec<Seen> = ElementQuery::from_root(ui)
        .match_descendants()
        .match_predicate(|e| e.accessible_label().is_some_and(|l| !l.is_empty()))
        .find_all()
        .into_iter()
        .filter_map(|e| {
            let text = e.accessible_label()?.to_string();
            if text.is_empty() {
                return None;
            }
            seen_in_window(ui, &e, text)
        })
        .collect();
    found.sort_by_key(|a| (a.y as i32, a.x as i32));
    // The same element comes up more than once (it is a child of every element above it).
    let mut known = std::collections::HashSet::new();
    found.retain(|s| known.insert((s.text.clone(), s.role.clone(), s.x as i32, s.y as i32)));
    found
}

fn inputs(ui: &MainWindow) -> Vec<(ElementHandle, Seen)> {
    let mut found: Vec<(ElementHandle, Seen)> = ElementQuery::from_root(ui)
        .match_descendants()
        .match_inherits("TextInput")
        .find_all()
        .into_iter()
        .filter_map(|e| {
            let value = e
                .accessible_value()
                .map(|v| v.to_string())
                .unwrap_or_default();
            let seen = seen_in_window(ui, &e, value)?;
            Some((e, seen))
        })
        .collect();
    found.sort_by_key(|a| (a.1.y as i32, a.1.x as i32));
    let mut known = std::collections::HashSet::new();
    found.retain(|(_, s)| known.insert((s.x as i32, s.y as i32)));
    found
}

fn json(list: &[Seen]) -> String {
    let rows: Vec<serde_json::Value> = list
        .iter()
        .map(|s| {
            serde_json::json!({
                "t": s.text, "x": s.x.round() as i32, "y": s.y.round() as i32,
                "w": s.w.round() as i32, "h": s.h.round() as i32,
                "r": s.role, "on": s.on,
            })
        })
        .collect();
    serde_json::Value::Array(rows).to_string()
}

fn at(seen: Option<&Seen>) -> String {
    match seen {
        Some(s) => format!("{} {}", s.x.round() as i32, s.y.round() as i32),
        None => "none".to_owned(),
    }
}

fn logical(ui: &MainWindow, x: f32, y: f32) -> LogicalPosition {
    let s = scale(ui);
    LogicalPosition::new(x / s, y / s)
}

/// A touch at a place of the window, in device pixels, through the same events the platform sends.
fn tap_at(ui: &MainWindow, x: f32, y: f32) {
    let position = logical(ui, x, y);
    let window = ui.window();
    window.dispatch_event(WindowEvent::PointerMoved { position });
    window.dispatch_event(WindowEvent::PointerPressed {
        position,
        button: PointerEventButton::Left,
    });
    window.dispatch_event(WindowEvent::PointerReleased {
        position,
        button: PointerEventButton::Left,
    });
}

fn key(ui: &MainWindow, text: slint::SharedString) {
    let window = ui.window();
    window.dispatch_event(WindowEvent::KeyPressed { text: text.clone() });
    window.dispatch_event(WindowEvent::KeyReleased { text });
}

/// Put `value` in a text field the way a person does: touch it, clear it, type it key by key, so the
/// field's own `edited` handler runs.
fn type_into(ui: &MainWindow, x: f32, y: f32, value: &str) {
    tap_at(ui, x, y);
    key(ui, Key::End.into());
    for _ in 0..80 {
        key(ui, Key::Backspace.into());
    }
    for c in value.chars() {
        key(ui, c.to_string().into());
    }
}

/// The commands. `None` when the command is not one of these.
pub fn run(ui: &MainWindow, command: &str) -> Option<String> {
    let (name, arg) = command.split_once(':').unwrap_or((command, ""));
    // `find@2:` is the third match, `find@L:` the last one.
    let (name, nth) = match name.split_once('@') {
        Some((name, "L")) => (name, usize::MAX),
        Some((name, n)) => (name, n.parse::<usize>().unwrap_or(0)),
        None => (name, 0),
    };
    Some(match name {
        "texts" => json(&texts(ui)),
        // The first text that says exactly this (`findc`: has it in it); `find@2:` takes the
        // third one. `tap` and `tapc` touch it.
        "find" | "findc" | "tap" | "tapc" => {
            let all = texts(ui);
            let loose = name.ends_with('c');
            let mut matching = all.iter().filter(|s| {
                if loose {
                    s.text.contains(arg)
                } else {
                    s.text == arg
                }
            });
            let hit = if nth == usize::MAX {
                matching.next_back()
            } else {
                matching.nth(nth)
            };
            if name.starts_with("tap") {
                match hit {
                    Some(s) => {
                        tap_at(ui, s.x, s.y);
                        "OK".to_owned()
                    }
                    None => "none".to_owned(),
                }
            } else {
                at(hit)
            }
        }
        // A touch at a place, in device pixels.
        "tapxy" => {
            let mut n = arg.split(' ').filter_map(|v| v.parse::<f32>().ok());
            match (n.next(), n.next()) {
                (Some(x), Some(y)) => {
                    tap_at(ui, x, y);
                    "OK".to_owned()
                }
                _ => "ERR tapxy: x y".to_owned(),
            }
        }
        // The elements of a component on screen (`boxes:AppSlider`): their middle and size.
        "boxes" => {
            let mut all: Vec<Seen> = ElementQuery::from_root(ui)
                .match_descendants()
                .match_inherits(arg)
                .find_all()
                .into_iter()
                .filter_map(|e| seen_in_window(ui, &e, arg.to_owned()))
                .collect();
            all.sort_by_key(|a| (a.y as i32, a.x as i32));
            json(&all)
        }
        "window" => {
            let size = ui.window().size();
            format!("{} {} {}", size.width, size.height, scale(ui))
        }
        "inputs" => {
            let all: Vec<Seen> = inputs(ui).into_iter().map(|(_, s)| s).collect();
            json(&all)
        }
        // `set:<n>:<text>`: the n-th text field on screen takes this text.
        "set" => match arg.split_once(':') {
            Some((n, value)) => match n
                .parse::<usize>()
                .ok()
                .and_then(|n| inputs(ui).into_iter().nth(n))
            {
                Some((element, _)) => {
                    element.set_accessible_value(value);
                    "OK".to_owned()
                }
                None => "none".to_owned(),
            },
            None => "ERR set: <n>:<text>".to_owned(),
        },
        // `settext:<n>:<text>`: the same through the keys.
        "settext" => match arg.split_once(':') {
            Some((n, value)) => match n
                .parse::<usize>()
                .ok()
                .and_then(|n| inputs(ui).into_iter().nth(n))
            {
                Some((_, seen)) => {
                    type_into(ui, seen.x, seen.y, value);
                    "OK".to_owned()
                }
                None => "none".to_owned(),
            },
            None => "ERR settext: <n>:<text>".to_owned(),
        },
        // What sticks out of the window at the sides, or has no room: the stops of a visual check.
        "overflow" => {
            let w = ui.window().size().width as f32;
            let all = texts(ui);
            let outside = |s: &Seen| s.x - s.w / 2.0 < -1.0 || s.x + s.w / 2.0 > w + 1.0;
            // A row of choices that scrolls sideways (the history's periods) goes past the edge by
            // design: what overflows on a line where several other things sit inside the window.
            let strip = |s: &Seen| {
                all.iter()
                    .filter(|o| (o.y - s.y).abs() < 6.0 && !outside(o) && o.text != s.text)
                    .count()
                    >= 3
            };
            let bad: Vec<String> = all
                .iter()
                .filter(|s| outside(s) && !strip(s))
                .map(|s| s.text.chars().take(24).collect())
                .collect();
            if bad.is_empty() {
                "none".to_owned()
            } else {
                bad.join(", ")
            }
        }
        _ => return None,
    })
}
