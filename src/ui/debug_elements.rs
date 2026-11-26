//! The part of the devtest channel that looks at the Slint elements: which texts are on screen
//! and where, the text fields, and a tap or drag at a place. Only with the cargo feature
//! `devtest` (a debug build made by `./build.sh`), never in a build for users: it needs the
//! compiler's debug info and the testing crate.

use i_slint_backend_testing::{ElementHandle, ElementQuery};
use slint::platform::{Key, PointerEventButton, WindowEvent};
use slint::{ComponentHandle, LogicalPosition};

use super::{Insets, MainWindow};

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
    /// Only for the text field that has the focus: where its cursor and selection are.
    caret: Option<Caret>,
}

/// The cursor and the other end of the selection of the focused text field, in characters, and the
/// middle of that field in device pixels (to tell which row of `inputs` it is).
struct Caret {
    text: String,
    cursor: usize,
    anchor: usize,
    preedit: String,
    x: f32,
    y: f32,
}

fn caret(ui: &MainWindow) -> Option<Caret> {
    use i_slint_core::items::TextInput;
    let inner = i_slint_core::window::WindowInner::from_pub(ui.window());
    let item = inner.focus_item.borrow().upgrade()?;
    let input = item.downcast::<TextInput>()?;
    let input = input.as_pin_ref();
    let text = input.text();
    let chars = |bytes: usize| text.get(..bytes).map_or(0, |t| t.chars().count());
    let geometry = item.geometry();
    let origin = item.map_to_native_window(geometry.origin);
    let s = scale(ui);
    Some(Caret {
        text: text.to_string(),
        cursor: chars(input.cursor_position(&text)),
        anchor: chars(input.anchor_position(&text)),
        preedit: input.preedit_text().to_string(),
        x: (origin.x + geometry.size.width / 2.0) * s,
        y: (origin.y + geometry.size.height / 2.0) * s,
    })
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
        caret: None,
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

/// Where everything that says something stands: the same twice in a row means the layout holds
/// still, also on a screen that is drawn all the time (a ring screen's backdrop moves).
pub(super) fn fingerprint(ui: &MainWindow) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for s in texts(ui) {
        (
            &s.text, s.x as i32, s.y as i32, s.w as i32, s.h as i32, s.on,
        )
            .hash(&mut h);
    }
    h.finish()
}

/// Is there a text on screen that says this (`exact`) or has it in it?
pub(super) fn has_text(ui: &MainWindow, wanted: &str, exact: bool) -> bool {
    texts(ui).iter().any(|s| {
        if exact {
            s.text == wanted
                || (s.role == "ListItem" && s.text.split(", ").any(|part| part == wanted))
        } else {
            s.text.contains(wanted)
        }
    })
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
    // The field with the focus carries its cursor and selection.
    if let Some(c) = caret(ui) {
        if let Some((_, seen)) = found
            .iter_mut()
            .find(|(_, s)| (s.x - c.x).abs() < 1.5 && (s.y - c.y).abs() < 1.5)
        {
            seen.caret = Some(c);
        }
    }
    found.sort_by_key(|a| (a.1.y as i32, a.1.x as i32));
    let mut known = std::collections::HashSet::new();
    found.retain(|(_, s)| known.insert((s.x as i32, s.y as i32)));
    found
}

fn json(list: &[Seen]) -> String {
    let rows: Vec<serde_json::Value> = list
        .iter()
        .map(|s| {
            let mut row = serde_json::json!({
                "t": s.text, "x": s.x.round() as i32, "y": s.y.round() as i32,
                "w": s.w.round() as i32, "h": s.h.round() as i32,
                "r": s.role, "on": s.on,
            });
            if let Some(c) = &s.caret {
                row["f"] = true.into();
                row["c"] = c.cursor.into();
                row["a"] = c.anchor.into();
                row["pre"] = c.preedit.clone().into();
            }
            row
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

thread_local!(static TRACE: std::cell::RefCell<std::collections::VecDeque<String>> = const { std::cell::RefCell::new(std::collections::VecDeque::new()) });

/// The last lines of what the window's touch handlers saw, with the time in ms since start.
const TRACE_LINES: usize = 400;

pub(super) fn trace(line: String) {
    TRACE.with(|t| {
        let mut t = t.borrow_mut();
        // The same line again (an input event every 16 ms) only counts.
        if let Some(last) = t.back_mut() {
            if let Some((_, said)) = last.split_once(' ') {
                let said = said.rsplit_once(" x").map_or(said, |(said, _)| said);
                if said == line {
                    let count = last
                        .rsplit_once(" x")
                        .and_then(|(_, n)| n.parse::<u32>().ok());
                    let head = last.split_once(' ').map_or("", |(ms, _)| ms).to_owned();
                    *last = format!("{head} {line} x{}", count.unwrap_or(1) + 1);
                    return;
                }
            }
        }
        if t.len() == TRACE_LINES {
            t.pop_front();
        }
        t.push_back(format!("{} {line}", super::ready::now_ms()));
    });
}

/// Every touch area that holds the point, bottom one first: the last one is what takes a press.
fn touch_areas_at(ui: &MainWindow, x: f32, y: f32) -> String {
    let s = scale(ui);
    ElementQuery::from_root(ui)
        .match_descendants()
        .match_inherits("TouchArea")
        .find_all()
        .into_iter()
        .filter_map(|e| {
            let (size, pos) = (e.size(), e.absolute_position());
            let (left, top) = (pos.x * s, pos.y * s);
            let inside =
                x >= left && x < left + size.width * s && y >= top && y < top + size.height * s;
            inside.then(|| {
                format!(
                    "{}x{}@{},{}",
                    (size.width * s) as i32,
                    (size.height * s) as i32,
                    left as i32,
                    top as i32
                )
            })
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// A touch at a place of the window, in device pixels, through the same events the platform sends.
fn tap_at(ui: &MainWindow, x: f32, y: f32) {
    trace(format!(
        "tap_at {x:.0},{y:.0} areas [{}]",
        touch_areas_at(ui, x, y)
    ));
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

/// Why a finger could not touch this place of the window, in device pixels: it is outside the window, or the keyboard is
/// over it (the window keeps its full height while the keyboard is up, so a place under it is inside the window).
fn out_of_reach(ui: &MainWindow, x: f32, y: f32) -> Option<String> {
    let win = ui.window().size();
    let (w, h) = (win.width as f32, win.height as f32);
    if !(x >= 0.0 && y >= 0.0 && x < w && y < h) {
        return Some(format!("ERR outside the window ({w}x{h}): {x} {y}"));
    }
    let keyboard = ui.global::<Insets>().get_bottom() * scale(ui);
    if keyboard > super::ready::KEYBOARD_MIN_PX && y >= h - keyboard {
        return Some(format!(
            "ERR under the keyboard: {x} {y}, it starts at {}",
            (h - keyboard).round()
        ));
    }
    None
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
    type_keys(ui, value);
}

/// The keys of a typing: to the end, the old text out, the new one in.
fn type_keys(ui: &MainWindow, value: &str) {
    key(ui, Key::End.into());
    for _ in 0..80 {
        key(ui, Key::Backspace.into());
    }
    for c in value.chars() {
        key(ui, c.to_string().into());
    }
}

thread_local! {
    static TAP_WAITS: std::cell::RefCell<Vec<std::rc::Rc<slint::Timer>>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// How often a held tap looks for its element again.
const TAP_WAIT_STEP: std::time::Duration = std::time::Duration::from_millis(30);
/// A tap that waited longer than this says so in its answer, so slow screens stay visible.
const TAP_WAIT_SAID: std::time::Duration = std::time::Duration::from_secs(1);

/// `tapwait:<ms>:<command>`, for a `tap`, `tapc` or `tapxy` command: the tap is made as soon as its element is on screen,
/// and when there is none after `<ms>` the answer is `none`, as for the command alone. Any other answer (the tap was made, or
/// a place a finger cannot reach, which waiting does not change) comes at once. The wait is held here, in one request, and
/// is no loop of requests from outside. An answer `OK` after more than a second is `OK waited <n>ms`.
pub fn start_tap_wait(
    ui: slint::Weak<MainWindow>,
    spec: &str,
    reply: impl FnOnce(String) + Send + 'static,
) {
    let (ms, command) = spec.split_once(':').unwrap_or((spec, ""));
    let name = command.split([':', '@']).next().unwrap_or("");
    let Ok(ms) = ms.parse::<u64>() else {
        reply(format!("ERR tapwait: <ms>:<command>, not {spec}"));
        return;
    };
    if !matches!(name, "tap" | "tapc" | "tapxy") {
        reply(format!("ERR tapwait waits for a tap, not for {name}"));
        return;
    }
    let (limit, command, started) = (
        std::time::Duration::from_millis(ms),
        command.to_owned(),
        std::time::Instant::now(),
    );
    let answer = move |ui: &MainWindow| -> Option<String> {
        let said = run(ui, &command)?;
        if said == "none" {
            return None;
        }
        let waited = started.elapsed();
        Some(if said == "OK" && waited >= TAP_WAIT_SAID {
            format!("OK waited {}ms", waited.as_millis())
        } else {
            said
        })
    };
    let Some(window) = ui.upgrade() else {
        reply("ERR no window".to_owned());
        return;
    };
    if let Some(said) = answer(&window) {
        reply(said);
        return;
    }
    let timer = std::rc::Rc::new(slint::Timer::default());
    let handle = timer.clone();
    let mut reply = Some(reply);
    timer.start(slint::TimerMode::Repeated, TAP_WAIT_STEP, move || {
        let said = match ui.upgrade() {
            None => Some("ERR no window".to_owned()),
            Some(window) => {
                answer(&window).or_else(|| (started.elapsed() >= limit).then(|| "none".to_owned()))
            }
        };
        if let Some(said) = said {
            handle.stop();
            if let Some(reply) = reply.take() {
                reply(said);
            }
        }
    });
    TAP_WAITS.with(|all| {
        let mut all = all.borrow_mut();
        all.retain(|t| t.running());
        all.push(timer);
    });
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
        // The trace lines (see `trace`), and an empty trace.
        "trace" => TRACE.with(|t| t.borrow().iter().cloned().collect::<Vec<_>>().join(" | ")),
        "trace_clear" => {
            TRACE.with(|t| t.borrow_mut().clear());
            "OK".to_owned()
        }
        // The readiness signals: first frame, screen, sheet and panel (see `ready`).
        "ready" => super::ready::state(ui),
        // The first text that says exactly this (`findc`: has it in it); `find@2:` takes the
        // third one. `tap` and `tapc` touch it.
        "find" | "findc" | "tap" | "tapc" => {
            let all = texts(ui);
            let loose = name.ends_with('c');
            let mut matching = all.iter().filter(|s| {
                if loose {
                    s.text.contains(arg)
                } else {
                    // An alarm card says "07:00, name, days": its name is found by itself.
                    s.text == arg
                        || (s.role == "ListItem" && s.text.split(", ").any(|part| part == arg))
                }
            });
            let hit = if nth == usize::MAX {
                matching.next_back()
            } else {
                matching.nth(nth)
            };
            if name.starts_with("tap") {
                match hit {
                    Some(s) => out_of_reach(ui, s.x, s.y).unwrap_or_else(|| {
                        tap_at(ui, s.x, s.y);
                        "OK".to_owned()
                    }),
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
                (Some(x), Some(y)) => out_of_reach(ui, x, y).unwrap_or_else(|| {
                    tap_at(ui, x, y);
                    "OK".to_owned()
                }),
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
        // The focused text field: its text, cursor and selection end (in characters), the preedit text, and its
        // middle in device pixels. `none` when no text field has the focus.
        "cursor" => match caret(ui) {
            Some(c) => serde_json::json!({
                "t": c.text, "c": c.cursor, "a": c.anchor, "pre": c.preedit,
                "x": c.x.round() as i32, "y": c.y.round() as i32,
            })
            .to_string(),
            None => "none".to_owned(),
        },
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
        // `settext:<n>:<text>`: the same through the keys. The answer is `OK <x> <y>`, the place of the field in device
        // pixels, for `heldat`.
        // `settext:focus:<text>` types into the field that has the focus, with no touch: a field shown for typing takes the focus by itself,
        // and a touch on the place it was read at can miss it when the sheet has moved since (the keyboard rising), which closed the sheet
        // (s38 on Android 16, 2026-11-26). `none` when no field has the focus.
        "settext" if arg.starts_with("focus:") => match caret(ui) {
            Some(c) => {
                type_keys(ui, &arg["focus:".len()..]);
                format!("OK {} {}", c.x.round() as i32, c.y.round() as i32)
            }
            None => "none".to_owned(),
        },
        // `settext:next:<text>`: the focus moves to the next field with the Tab key, and the text is typed into that one: no touch either.
        // `none` when no field has the focus, or when Tab did not move it.
        "settext" if arg.starts_with("next:") => match caret(ui) {
            Some(before) => {
                key(ui, Key::Tab.into());
                match caret(ui) {
                    Some(c) if (c.x - before.x).abs() > 1.5 || (c.y - before.y).abs() > 1.5 => {
                        type_keys(ui, &arg["next:".len()..]);
                        format!("OK {} {}", c.x.round() as i32, c.y.round() as i32)
                    }
                    _ => "none".to_owned(),
                }
            }
            None => "none".to_owned(),
        },
        "settext" => match arg.split_once(':') {
            Some((n, value)) => match n
                .parse::<usize>()
                .ok()
                .and_then(|n| inputs(ui).into_iter().nth(n))
            {
                Some((_, seen)) => {
                    type_into(ui, seen.x, seen.y, value);
                    format!("OK {} {}", seen.x.round() as i32, seen.y.round() as i32)
                }
                None => "none".to_owned(),
            },
            None => "ERR settext: <n>:<text>".to_owned(),
        },
        // `heldat:<x>:<y>`: what the field that was typed into holds now: the field that has the focus, or else the one
        // at that place (the app builds a field again for a new text and the focus is gone for a moment, and the keyboard
        // moves the sheet, so the number of the field says nothing). `TEXT:<text>`, or `GONE` when neither is there (the
        // last key of a challenge's text takes its field away).
        "heldat" => {
            let held = caret(ui).map(|c| c.text).or_else(|| {
                let (x, y) = arg.split_once(':')?;
                let (x, y) = (x.parse::<f32>().ok()?, y.parse::<f32>().ok()?);
                inputs(ui)
                    .into_iter()
                    .find(|(_, s)| (s.x - x).abs() < 3.0 && (s.y - y).abs() < 3.0)
                    .map(|(_, s)| s.text)
            });
            match held {
                Some(text) => format!("TEXT:{text}"),
                None => "GONE".to_owned(),
            }
        }
        // What sticks out of the window at the sides, has no room, or is a chip cut short: the stops of a visual check.
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
            let mut bad: Vec<String> = all
                .iter()
                .filter(|s| outside(s) && !strip(s))
                .map(|s| s.text.chars().take(24).collect())
                .collect();
            // A chip whose text is cut short with an ellipsis (`ChoiceChips` says so in its description).
            bad.extend(
                ElementQuery::from_root(ui)
                    .match_descendants()
                    .match_predicate(|e| e.accessible_description().is_some_and(|d| d == "elided"))
                    .find_all()
                    .into_iter()
                    .filter_map(|e| {
                        let text = e.accessible_label()?.to_string();
                        seen_in_window(ui, &e, text)
                    })
                    .map(|s| format!("elided {}", s.text.chars().take(24).collect::<String>())),
            );
            bad.sort();
            bad.dedup();
            if bad.is_empty() {
                "none".to_owned()
            } else {
                bad.join(", ")
            }
        }
        // Every element of the window that is wider than where it sits: past the window at the
        // sides (also when its middle is outside), or past the sides of a `Component` it is part of
        // (`fit:AlarmCard`, `fit:RowList`; no argument: only the window). `none` when all is inside.
        "fit" => {
            let win = ui.window().size().width as f32 / scale(ui);
            let drawn = |e: &ElementHandle| {
                let size = e.size();
                size.width > 0.5 && size.height > 0.5 && e.computed_opacity() > 0.01
            };
            let mut bad: Vec<String> = Vec::new();
            let name = |e: &ElementHandle| {
                e.accessible_label()
                    .filter(|l| !l.is_empty())
                    .or_else(|| e.type_name())
                    .unwrap_or_default()
                    .chars()
                    .take(24)
                    .collect::<String>()
            };
            for e in ElementQuery::from_root(ui)
                .match_descendants()
                .match_predicate(|e| e.accessible_label().is_some_and(|l| !l.is_empty()))
                .find_all()
            {
                let (x, w) = (e.absolute_position().x, e.size().width);
                if drawn(&e) && (x < -1.0 || x + w > win + 1.0) {
                    bad.push(format!("{} past the window", name(&e)));
                }
            }
            if !arg.is_empty() {
                for parent in ElementQuery::from_root(ui)
                    .match_descendants()
                    .match_inherits(arg)
                    .find_all()
                {
                    if !drawn(&parent) {
                        continue;
                    }
                    let (left, right) = (
                        parent.absolute_position().x,
                        parent.absolute_position().x + parent.size().width,
                    );
                    for e in parent.query_descendants().match_descendants().find_all() {
                        let (x, w) = (e.absolute_position().x, e.size().width);
                        if drawn(&e) && (x < left - 1.0 || x + w > right + 1.0) {
                            bad.push(format!("{} past {arg}", name(&e)));
                        }
                    }
                }
            }
            bad.sort();
            bad.dedup();
            if bad.is_empty() {
                "none".to_owned()
            } else {
                bad.join(", ")
            }
        }
        _ => return None,
    })
}
