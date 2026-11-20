//! Every panel and screen, reached by touching what a person touches, on a window as wide as a
//! phone's: every panel opens, every choice in it takes and the previous one comes back, and nothing
//! sticks out of the window at the sides or lies over something else. The same walk as the emulator
//! scenario s57 (devtest/panels.sh), through the same accessible labels, in seconds. The emulator
//! keeps a short s57 for what only a real screen shows.

use slint::Model;
use std::cell::RefCell;
use std::rc::Rc;

use super::a11y_tests::alarm;
use super::*;

/// One element of the accessible tree, in device pixels (the middle and the size).
#[derive(Clone, Debug)]
struct El {
    t: String,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    role: String,
    on: Option<bool>,
}

/// How the window is: size in device pixels, density (scale factor) and the phone's text size.
#[derive(Clone, Copy, Debug)]
struct Look {
    name: &'static str,
    width: u32,
    height: u32,
    scale: f32,
    font: f32,
}

/// The emulator suite's phone (1080x2376 at 480 dpi) and the two large sizes of s65. The window is
/// tall so that everything is reachable without scrolling; what sticks out at the sides does not
/// depend on the height.
const LOOKS: [Look; 4] = [
    Look {
        name: "phone",
        width: 1080,
        height: 9000,
        scale: 3.0,
        font: 1.0,
    },
    Look {
        name: "phone big text",
        width: 1080,
        height: 9000,
        scale: 3.0,
        font: 1.3,
    },
    Look {
        name: "large 560 dpi",
        width: 1080,
        height: 9000,
        scale: 3.5,
        font: 1.3,
    },
    Look {
        name: "large 640 dpi",
        width: 1080,
        height: 9000,
        scale: 4.0,
        font: 1.15,
    },
];

/// Found by this test and left as it is: (look, screen, text) that sticks out of the window.
const KNOWN_OUTSIDE: [(&str, &str, &str); 0] = [];

/// Choices that open the phone's picker (s57 only touches them on a real phone).
const PICKERS: [&str; 1] = ["Photo"];

struct Tour {
    ui: MainWindow,
    ctx: Rc<Ctx>,
    look: Look,
    problems: RefCell<Vec<String>>,
    /// Names of the panels and screens that were opened and looked through.
    opened: RefCell<Vec<String>>,
    /// How many choices were taken and taken back.
    taken: RefCell<usize>,
    /// Whether the choices of a home or settings panel count as on or off, from what the panel
    /// holds: only for panels where taking a choice does nothing else (not delete, pause, restore).
    choice_state: std::cell::Cell<bool>,
    /// What the tree said since the last touch: asking again would say the same.
    cache: RefCell<Option<Vec<El>>>,
}

fn work_alarm() -> crate::models::AlarmData {
    let mut a = alarm("work", 7, "Work");
    a.note = "Take the laptop charger".into();
    a.challenges = [
        ("challenge.math", r#"{"difficulty":"medium"}"#),
        ("challenge.typing", r#"{"difficulty":"easy"}"#),
        ("challenge.memory", r#"{"difficulty":"easy"}"#),
        (
            "challenge.reaction",
            r#"{"count":10,"time_ms":3000,"rounds":1}"#,
        ),
        ("challenge.hold", r#"{"count":1,"hold_ms":3000}"#),
        ("challenge.steps", r#"{"count":20}"#),
        ("challenge.shake", r#"{"count":5}"#),
        ("challenge.scan", r#"{"value":"12345"}"#),
    ]
    .iter()
    .map(|(a, b)| (a.to_string(), b.to_string()))
    .collect();
    a
}

impl Tour {
    fn new(look: Look) -> Tour {
        let mut gym = alarm("gym", 18, "Gym");
        gym.enabled = false;
        let mut long = alarm("long", 9, "A very long label that keeps going on and on");
        long.days = [false, false, false, false, false, true, true];
        Tour::with_alarms(look, vec![work_alarm(), long, gym])
    }

    fn with_alarms(look: Look, alarms: Vec<crate::models::AlarmData>) -> Tour {
        thread_local!(static INIT: () = i_slint_backend_testing::init_no_event_loop());
        INIT.with(|_| {});
        let ui = MainWindow::new().unwrap();
        ui.window()
            .dispatch_event(slint::platform::WindowEvent::ScaleFactorChanged {
                scale_factor: look.scale,
            });
        ui.window()
            .set_size(slint::PhysicalSize::new(look.width, look.height));
        ui.global::<Motion>().set_less(true);
        ui.global::<Zoom>().set_scale(look.font);
        let mut settings = AppSettings::default();
        settings.prefs.less_motion = true;
        let ctx = new_ctx(&ui, settings, alarms);
        let weak = Rc::downgrade(&ctx);
        ui.global::<Tr>().on_lookup(move |key, _| {
            weak.upgrade()
                .map(|ctx| i18n::t(&ctx.tr.borrow(), key.as_str()))
                .unwrap_or_else(|| key.to_string())
                .into()
        });
        connect_layout(&ui);
        connect(&ctx, &ui);
        ui.show().unwrap();
        home::refresh(&ctx);
        Tour {
            ui,
            ctx,
            look,
            problems: RefCell::new(Vec::new()),
            opened: RefCell::new(Vec::new()),
            taken: RefCell::new(0),
            choice_state: std::cell::Cell::new(false),
            cache: RefCell::new(None),
        }
    }

    fn problem(&self, msg: impl std::fmt::Display) {
        self.problems
            .borrow_mut()
            .push(format!("[{}] {msg}", self.look.name));
    }

    fn cmd(&self, command: &str) -> String {
        let said = debug_elements::run(&self.ui, command).unwrap();
        // What a touch sets going (a panel that closes after a moment) gets its time.
        if command.starts_with("tap") {
            self.settle();
        }
        said
    }

    /// Time passes after a touch, and what was read is no longer true.
    fn settle(&self) {
        i_slint_backend_testing::mock_elapsed_time(std::time::Duration::from_millis(800));
        *self.cache.borrow_mut() = None;
    }

    fn els(&self) -> Vec<El> {
        if let Some(cached) = self.cache.borrow().as_ref() {
            return cached.clone();
        }
        let found = self.read();
        *self.cache.borrow_mut() = Some(found.clone());
        found
    }

    fn read(&self) -> Vec<El> {
        let rows: Vec<serde_json::Value> = serde_json::from_str(&self.cmd("texts")).unwrap();
        let mut state: Vec<(String, bool)> = Vec::new();
        if self.choice_state.get() {
            if self.ui.get_home_sheet() != HomeSheet::None {
                state.extend(
                    self.ui
                        .get_hs_choices()
                        .iter()
                        .map(|c| (c.text.to_string(), c.selected)),
                );
            }
            if self.ui.get_panel_open() {
                state.extend(
                    self.ui
                        .get_pn_choices()
                        .iter()
                        .map(|c| (c.text.to_string(), c.selected)),
                );
                let on = self.ui.get_pn_chip_selected();
                state.extend(
                    self.ui
                        .get_pn_chips()
                        .iter()
                        .enumerate()
                        .map(|(i, c)| (c.to_string(), i as i32 == on)),
                );
            }
        }
        rows.iter()
            .map(|r| El {
                t: r["t"].as_str().unwrap().to_owned(),
                x: r["x"].as_f64().unwrap(),
                y: r["y"].as_f64().unwrap(),
                w: r["w"].as_f64().unwrap(),
                h: r["h"].as_f64().unwrap(),
                role: r["r"].as_str().unwrap().to_owned(),
                on: r["on"].as_bool().or_else(|| {
                    (r["r"] == "Button")
                        .then(|| {
                            state
                                .iter()
                                .find(|(t, _)| t == r["t"].as_str().unwrap())
                                .map(|(_, on)| *on)
                        })
                        .flatten()
                }),
            })
            .collect()
    }

    /// The element that says this (exactly, or has it in it), the `nth` one by place; the last
    /// when `nth` is `usize::MAX`. An alarm card is found by each part of what it says.
    fn find(&self, label: &str, contains: bool, nth: usize) -> Option<El> {
        let els = self.els();
        let mut hits = els.into_iter().filter(|e| {
            if contains {
                e.t.contains(label)
            } else {
                e.t == label || (e.role == "ListItem" && e.t.split(", ").any(|p| p == label))
            }
        });
        if nth == usize::MAX {
            hits.next_back()
        } else {
            hits.nth(nth)
        }
    }
    /// Touch the element that says exactly this; false when nothing does.
    fn tap(&self, label: &str) -> bool {
        self.find(label, false, 0).is_some_and(|e| self.tap_el(&e))
    }
    /// Touch where the element is, not at the first one that says the same.
    fn tap_el(&self, e: &El) -> bool {
        self.cmd(&format!("tapxy:{} {}", e.x, e.y)) == "OK"
    }
    fn tap_nth(&self, label: &str, n: usize) -> bool {
        self.find(label, false, n).is_some_and(|e| self.tap_el(&e))
    }
    fn tapc(&self, label: &str) -> bool {
        self.find(label, true, 0).is_some_and(|e| self.tap_el(&e))
    }
    fn has(&self, label: &str) -> bool {
        self.find(label, false, 0).is_some()
    }
    fn press(&self, name: &str, arg: &str) {
        let said = super::debug::run(&self.ctx, &format!("press:{name}:{arg}"));
        assert_eq!(said, "OK", "press {name}");
        self.settle();
    }
    fn back(&self) {
        super::back(&self.ctx);
        self.settle();
    }

    fn sheet_up(&self) -> bool {
        self.ui.get_home_sheet() != HomeSheet::None
            || self.ui.get_panel_open()
            || self.ui.get_ed_sheet() != EditorSheet::None
            || self.ui.global::<Thm>().get_sheet_open()
    }

    /// What the screen under a panel has that can be on or off: a panel's scrim leaves it in the tree.
    fn baseline(&self) -> Vec<(String, String)> {
        self.els().into_iter().map(|e| (e.t, e.role)).collect()
    }

    /// The things on screen that are on or off, one per text, without what is `under` the panel.
    fn checkables(&self, under: &[(String, String)]) -> Vec<El> {
        let mut seen: Vec<El> = Vec::new();
        for e in self.els() {
            if e.on.is_none() || e.role == "Tab" || e.t.starts_with("What is this?") {
                continue;
            }
            if under.contains(&(e.t.clone(), e.role.clone())) || seen.iter().any(|s| s.t == e.t) {
                continue;
            }
            seen.push(e);
        }
        seen
    }

    /// Text that sticks out of the window, an element wider than where it sits, and elements that lie
    /// over each other (neither one inside the other).
    fn layout(&self, name: &str, under: &[(String, String)]) {
        let all = self.els();
        // Text out of the window at the sides; a strip that scrolls sideways (the periods of the
        // history) goes past the edge by design: what overflows on a line where two others sit inside.
        let width = f64::from(self.look.width);
        let outside = |s: &El| s.x - s.w / 2.0 < -1.0 || s.x + s.w / 2.0 > width + 1.0;
        let strip = |s: &El| {
            all.iter()
                .filter(|o| (o.y - s.y).abs() < 6.0 && !outside(o) && o.t != s.t)
                .count()
                >= 2
        };
        for s in all.iter().filter(|s| outside(s) && !strip(s)) {
            if KNOWN_OUTSIDE.iter().any(|(look, screen, text)| {
                *look == self.look.name && name.starts_with(screen) && *text == s.t
            }) {
                continue;
            }
            let said: String = s.t.chars().take(24).collect();
            self.problem(format!(
                "{name}: '{said}' sticks out of the window (from {:.0} to {:.0} of {width:.0})",
                s.x - s.w / 2.0,
                s.x + s.w / 2.0
            ));
        }
        // Anything wider than where it sits, also when its middle is outside the window, which the
        // texts above do not show (s65 asks the same through `fit`).
        if !name.ends_with(" help") {
            let scale = f64::from(self.ui.window().scale_factor());
            for (text, line) in self.past_window() {
                let on_a_strip = all
                    .iter()
                    .filter(|o| (o.y - line * scale).abs() < 20.0 && o.t != text)
                    .count()
                    >= 2;
                if !on_a_strip
                    && !KNOWN_OUTSIDE.iter().any(|(look, screen, t)| {
                        *look == self.look.name && name.starts_with(screen) && *t == text
                    })
                {
                    self.problem(format!("{name}: '{text}' is wider than the window"));
                }
            }
        }
        let els: Vec<El> = all
            .into_iter()
            .filter(|e| !under.contains(&(e.t.clone(), e.role.clone())))
            .filter(|e| e.role != "Tab")
            .filter(|e| {
                matches!(
                    e.role.as_str(),
                    "Button" | "Switch" | "RadioButton" | "Tab" | "ListItem"
                )
            })
            .collect();
        let rect = |e: &El| {
            (
                e.x - e.w / 2.0,
                e.x + e.w / 2.0,
                e.y - e.h / 2.0,
                e.y + e.h / 2.0,
            )
        };
        let inside = |a: (f64, f64, f64, f64), b: (f64, f64, f64, f64)| {
            a.0 >= b.0 - 3.0 && a.1 <= b.1 + 3.0 && a.2 >= b.2 - 3.0 && a.3 <= b.3 + 3.0
        };
        for (i, a) in els.iter().enumerate() {
            for b in &els[i + 1..] {
                let (ra, rb) = (rect(a), rect(b));
                let ox = ra.1.min(rb.1) - ra.0.max(rb.0);
                let oy = ra.3.min(rb.3) - ra.2.max(rb.2);
                if ox > 6.0 && oy > 6.0 && !inside(ra, rb) && !inside(rb, ra) {
                    self.problem(format!("{name}: '{}' and '{}' overlap", a.t, b.t));
                }
            }
        }
    }

    /// What lies past the sides of the window, with the line it is on (logical px).
    fn past_window(&self) -> Vec<(String, f64)> {
        let window = self.ui.window();
        let win = window.size().width as f32 / window.scale_factor();
        let mut found: Vec<(String, f64)> = Vec::new();
        for e in i_slint_backend_testing::ElementQuery::from_root(&self.ui)
            .match_descendants()
            .match_predicate(|e| e.accessible_label().is_some_and(|l| !l.is_empty()))
            .find_all()
        {
            let size = e.size();
            if size.width <= 0.5 || size.height <= 0.5 || e.computed_opacity() <= 0.01 {
                continue;
            }
            let at = e.absolute_position();
            if at.x < -1.0 || at.x + size.width > win + 1.0 {
                let text: String = e
                    .accessible_label()
                    .unwrap_or_default()
                    .chars()
                    .take(24)
                    .collect();
                found.push((text, f64::from(at.y + size.height / 2.0)));
            }
        }
        found.sort_by(|a, b| a.0.cmp(&b.0));
        found.dedup_by(|a, b| a.0 == b.0);
        found
    }

    fn opened(&self, name: &str) {
        self.opened.borrow_mut().push(name.to_owned());
    }

    /// Choices of the open panel: each one is taken (it is marked, a switch switches), then what was
    /// on is taken again, and the panel is as it was. `reopen` opens the panel again when a pick closes
    /// it. Returns false when the panel is closed now.
    fn try_chips(
        &self,
        name: &str,
        under: &[(String, String)],
        reopen: &dyn Fn(&Tour) -> bool,
    ) -> bool {
        let before = self.checkables(under);
        if before.is_empty() {
            return true;
        }
        for c in &before {
            // The phone's own picker opens for these: nothing to take here.
            if PICKERS.contains(&c.t.as_str()) {
                continue;
            }
            // A radio button comes back only by taking another one of its group: without one that is
            // on (the list of sounds is read from the phone's files, and empty here) it stays.
            if c.role == "RadioButton"
                && !before
                    .iter()
                    .any(|b| b.role == "RadioButton" && b.on == Some(true))
            {
                continue;
            }
            let Some(here) = self.checkables(under).into_iter().find(|e| e.t == c.t) else {
                continue;
            };
            if !self.tap_el(&here) {
                self.problem(format!("{name}: '{}' cannot be touched", c.t));
                continue;
            }
            if std::env::var_os("TOUR_TRACE").is_some() {
                println!(
                    "    {name}: tapped {:?} ({:?}), sheet up {}, now {:?}",
                    c.t,
                    c.on,
                    self.sheet_up(),
                    self.checkables(under)
                        .iter()
                        .map(|e| (e.t.clone(), e.on))
                        .collect::<Vec<_>>()
                );
            }
            if !self.sheet_up() {
                if c.role == "Switch" {
                    self.problem(format!("{name}: the switch '{}' closes the panel", c.t));
                }
                if !reopen(self) {
                    self.problem(format!(
                        "{name}: the panel does not open again after '{}'",
                        c.t
                    ));
                    return false;
                }
                let now = self
                    .checkables(under)
                    .into_iter()
                    .find(|e| e.t == c.t)
                    .and_then(|e| e.on);
                if now != Some(true) {
                    self.problem(format!("{name}: the choice '{}' does not take", c.t));
                }
                *self.taken.borrow_mut() += 1;
                // The next choice is taken from the panel as it is now; what was on comes back at the end.
                continue;
            }
            let now = self
                .checkables(under)
                .into_iter()
                .find(|e| e.t == c.t)
                .and_then(|e| e.on);
            if c.role == "Switch" {
                if now == c.on {
                    self.problem(format!("{name}: the switch '{}' does not switch", c.t));
                }
                if let Some(here) = self.checkables(under).into_iter().find(|e| e.t == c.t) {
                    self.tap_el(&here);
                }
            } else if now != Some(true) {
                self.problem(format!(
                    "{name}: the choice '{}' does not take (marked: {now:?})",
                    c.t
                ));
            }
            *self.taken.borrow_mut() += 1;
        }
        // Choices go back by taking what was on, last.
        for b in before
            .iter()
            .filter(|b| b.on == Some(true) && b.role != "Switch")
        {
            let now = self.checkables(under);
            if let Some(here) = now.iter().find(|e| e.t == b.t && e.on != Some(true)) {
                self.tap_el(here);
                if std::env::var_os("TOUR_TRACE").is_some() {
                    println!("    {name}: back to {:?}", b.t);
                }
            }
        }
        // A radio button that was off toggles back by itself.
        for b in before
            .iter()
            .filter(|b| b.role == "RadioButton" && b.on != Some(true))
        {
            if let Some(here) = self
                .checkables(under)
                .iter()
                .find(|e| e.t == b.t && e.on == Some(true))
            {
                self.tap_el(here);
            }
        }
        // A pick that closes the panel: look at it again.
        if !self.sheet_up() && !reopen(self) {
            self.problem(format!(
                "{name}: the panel does not open again after taking the choices back"
            ));
            return false;
        }
        let any_on = before.iter().any(|b| b.on == Some(true));
        let automatic =
            matches!(name, "theme_own_Cards" | "theme_own_Text") && self.has("Automatic");
        // A colour that was automatic (nothing ringed) goes back by Automatic: a swatch cannot be taken off.
        if !any_on && automatic {
            self.tap_nth("Automatic", usize::MAX);
            // It can close the panel: look at it again.
            if !self.sheet_up() && !reopen(self) {
                self.problem(format!(
                    "{name}: the panel does not open again after Automatic"
                ));
                return false;
            }
        }
        // A colour of one's own that is none of the swatches has nothing to go back to: then only
        // what is there counts, not what is on.
        let state = any_on || automatic;
        let key = |l: &[El]| {
            let mut v: Vec<(String, String, Option<bool>)> = l
                .iter()
                .map(|e| (e.t.clone(), e.role.clone(), if state { e.on } else { None }))
                .collect();
            v.sort();
            v
        };
        let after = self.checkables(under);
        if key(&after) != key(&before) {
            self.problem(format!(
                "{name}: the choices do not go back: was {:?}, is {:?}",
                key(&before),
                key(&after)
            ));
        }
        true
    }

    /// The ? on top, if there is one: it opens the explanation in place, and closes it again.
    fn help(&self, name: &str, under: &[(String, String)]) {
        if !self.has("What is this?") {
            return;
        }
        let before = self.layout_sig();
        self.tap("What is this?");
        if self.layout_sig() == before {
            self.problem(format!("{name}: the ? opens nothing"));
        }
        self.layout(&format!("{name} help"), under);
        self.tap("What is this?");
        if self.layout_sig() != before {
            self.problem(format!("{name}: the ? does not close"));
        }
    }

    /// Where things are, without what they say.
    fn layout_sig(&self) -> Vec<(i64, i64)> {
        let mut v: Vec<(i64, i64)> = self
            .els()
            .iter()
            .map(|e| (e.x as i64, e.y as i64))
            .collect();
        v.sort();
        v
    }

    /// A panel that just opened (`reopen` opens it again): all of it, and what closes it closes it
    /// and nothing else.
    fn panel(&self, name: &str, under: &[(String, String)], reopen: &dyn Fn(&Tour) -> bool) {
        if !self.sheet_up() {
            self.problem(format!("{name}: no panel"));
            return;
        }
        self.opened(name);
        self.layout(name, under);
        self.help(name, under);
        if name.contains("Language") {
            self.try_language(name, reopen);
            return;
        }
        if !self.try_chips(name, under, reopen) {
            return;
        }
        let done = self.has("Done");
        if done {
            self.tap("Done");
        } else {
            self.back();
        }
        if self.sheet_up() {
            self.problem(format!(
                "{name}: {} leaves it open",
                if done { "Done" } else { "Back" }
            ));
            self.back();
        }
    }

    /// A language closes the sheet. One other one takes, and English comes back.
    fn try_language(&self, name: &str, reopen: &dyn Fn(&Tour) -> bool) {
        self.tap("Nederlands");
        if !self.has("Instellingen") {
            self.problem(format!("{name}: Nederlands does not take"));
        }
        if self.sheet_up() {
            self.problem(format!("{name}: choosing a language leaves the sheet open"));
            self.back();
        }
        if !reopen(self) {
            self.problem(format!("{name}: the sheet does not open again"));
            return;
        }
        self.layout("language in Dutch", &[]);
        self.tap_nth("English", usize::MAX);
        if !self.has("Settings") {
            self.problem(format!("{name}: English does not come back"));
        }
        if self.sheet_up() {
            self.problem(format!("{name}: choosing English leaves the sheet open"));
            self.back();
        }
    }

    /// The screen on top, looked at, with its help.
    fn screen(&self, name: &str) {
        self.opened(name);
        self.layout(name, &[]);
        // What s65 looks at: the cards, the quick row, the days and the rows of the editor.
        if name.starts_with("home") || name.starts_with("editor") {
            for check in ["fit:AlarmCard", "fit:RowList"] {
                let said = self.cmd(check);
                if said != "none" {
                    self.problem(format!("{name}: {said}"));
                }
            }
        }
        self.help(name, &[]);
    }

    /// Open a panel with `open` (which says whether one came up) and go through it.
    fn open_panel(&self, name: &str, open: &dyn Fn(&Tour) -> bool) {
        let under = self.baseline();
        if !open(self) {
            self.problem(format!("{name}: nothing opens it"));
            return;
        }
        let start = std::time::Instant::now();
        self.panel(name, &under, open);
        if std::env::var_os("TOUR_TRACE").is_some() {
            println!("  {} {name}: {:?}", self.look.name, start.elapsed());
        }
    }

    /// The panel that a touch on the text `label` (contains, the `nth` one) opens.
    fn row_panel(&self, name: &str, label: &str, nth: usize) {
        self.open_panel(name, &|t: &Tour| {
            t.cmd(&format!("tapc@{nth}:{label}")) == "OK" && t.sheet_up()
        });
    }

    /// Back to the list from the editor: a changed alarm asks first, and Discard is the answer.
    fn leave_editor(&self) {
        let under = self.baseline();
        self.back();
        if self.sheet_up() {
            self.opened("editor_discard");
            self.layout("editor_discard", &under);
            self.tap("Discard");
        }
        if self.ui.get_screen() != 0 || self.sheet_up() {
            self.problem("not back on the list from the editor");
            self.ui.set_ed_sheet(EditorSheet::None);
            self.ui.set_screen(0);
        }
    }

    fn is_on(&self, label: &str) -> bool {
        self.els()
            .iter()
            .any(|e| e.t == label && e.on == Some(true))
    }
}

fn editor_walk(t: &Tour) {
    // A new alarm.
    t.tap("Create new alarm");
    if t.ui.get_screen() != 1 {
        t.problem("no editor for a new alarm");
        return;
    }
    t.screen("editor_new");
    t.row_panel("editor_date", "One date", 0);
    t.leave_editor();

    // The alarm with everything on it.
    t.press("open", "work");
    if t.ui.get_screen() != 1 {
        t.problem("no editor");
        return;
    }
    t.screen("editor");
    // The small steps either side of the time: a minute later, and back.
    let before = t.ui.get_ed_time_text().to_string();
    t.tap("+1");
    let after = t.ui.get_ed_time_text().to_string();
    t.tap("−1");
    if before == after || t.ui.get_ed_time_text() != before.as_str() {
        t.problem("the minute buttons do not move the time a minute and back");
    }
    t.row_panel("editor_time", "Time:", 0);
    let trash = |t: &Tour| {
        let title = t.ui.get_ed_title().to_string();
        let Some(y) = t.els().iter().find(|e| e.t == title).map(|e| e.y) else {
            return false;
        };
        let x = f64::from(t.look.width) - 22.0 * f64::from(t.look.scale);
        t.cmd(&format!("tapxy:{x} {y}")) == "OK" && t.sheet_up()
    };
    t.open_panel("editor_delete", &trash);
    for name in [
        "Label",
        "Sound",
        "Vibration",
        "Snooze",
        "Alarm screen",
        "Ring limit",
        "Are you up?",
        "Reminder",
    ] {
        t.row_panel(&format!("editor_{name}"), name, 0);
    }
    try_out_walk(t);
    ring_look_walk(t);
    challenges_walk(t);
    t.leave_editor();
}

/// The challenges panel: adding one, trying them all.
fn challenges_walk(t: &Tour) {
    let open = |t: &Tour| t.cmd("tapc:Challenges") == "OK" && t.sheet_up();
    t.open_panel("editor_Challenges", &open);
    if !open(t) {
        return;
    }
    if !t.has("Try all") {
        t.problem("no way to try them all");
    }
    let under = t.baseline();
    if t.tap("Add a challenge") && t.ui.get_ed_sheet() == EditorSheet::AddChallenge {
        t.opened("challenge_add");
        t.layout("challenge_add", &under);
        t.help("challenge_add", &under);
        t.back();
        if t.ui.get_ed_sheet() == EditorSheet::None {
            t.problem("Back from adding a challenge leaves the challenges too");
            open(t);
        }
    } else {
        t.problem("no way to add a challenge");
    }
    let under = t.baseline();
    t.tap("Try all");
    if t.ui.global::<Run>().get_active() {
        t.opened("challenge_run");
        t.layout("challenge_run", &under);
        if t.has("Can't do this one? Hold instead") {
            let under = t.baseline();
            t.tap("Can't do this one? Hold instead");
            t.layout("challenge_other_way", &under);
        }
        t.back();
        t.back();
    } else {
        t.problem("a challenge cannot be tried");
    }
    if t.sheet_up() {
        t.back();
    }
}

/// The look of the ring screen, as big as it will be, for every look and clock there is.
fn ring_look_walk(t: &Tour) {
    let open = |t: &Tour| t.cmd("tapc:Alarm screen") == "OK" && t.sheet_up();
    if !open(t) {
        t.problem("no panel for the ring screen");
        return;
    }
    let preview = |t: &Tour, name: &str| {
        let under = t.baseline();
        t.tap_nth("Preview", usize::MAX);
        if t.ui.get_ed_preview() {
            t.layout(name, &under);
            t.tap("Preview. Tap to close.");
            if t.ui.get_ed_preview() {
                t.problem(format!("{name}: not back from the preview"));
                t.back();
            }
        } else {
            t.problem(format!("{name}: no preview"));
        }
    };
    for name in ["Aurora", "Sunrise", "Waves", "Stars", "Rings", "Bubbles"] {
        t.tap(name);
        preview(t, &format!("ring_look_{name}"));
    }
    let under = t.baseline();
    if t.cmd("tapc:Colour") == "OK" && t.has("Colour code") {
        t.layout("ring_look_colour", &under);
        t.tap("Done");
        preview(t, "ring_look_own_colour");
    } else {
        t.problem("no colour for the ring screen");
    }
    for name in ["Thin", "Bold", "Classic", "Mono"] {
        t.tap(name);
        preview(t, &format!("ring_clock_{name}"));
    }
    t.tap("Plain");
    t.tap("Clean");
    t.back();
    if t.sheet_up() {
        t.problem("Back leaves the ring screen's panel open");
        t.back();
    }
}

/// Sound and vibration: trying one shows a way to stop it.
fn try_out_walk(t: &Tour) {
    let sound = |t: &Tour| t.cmd("tapc:Sound") == "OK" && t.sheet_up();
    let under = t.baseline();
    if sound(t) && t.has("Play preview") {
        // The sounds are files on the phone and there is no audio here: playing one is for the emulator.
        t.tap("Play preview");
        t.layout("editor_sound_playing", &under);
    } else {
        t.problem("no way to try a sound");
    }
    if t.sheet_up() {
        t.back();
    }
    let vibration = |t: &Tour| t.cmd("tapc:Vibration") == "OK" && t.sheet_up();
    if vibration(t) && t.has("Steady") {
        t.tap("Steady");
        if !t.has("Stop preview") {
            t.problem("a tap on a pattern shows no way to stop it");
        }
        t.layout("editor_vibration_running", &under);
    } else {
        t.problem("no vibration panel");
    }
    if t.sheet_up() {
        t.back();
    }
}

fn theme_walk(t: &Tour) {
    t.screen("theme");
    for name in [
        "Same as the phone",
        "Dark",
        "Light",
        "Pure black",
        "Midnight",
        "Forest",
        "Plum",
        "Sand",
        "Mist",
    ] {
        t.tap(name);
        if !t.is_on(name) {
            t.problem(format!("theme {name} does not take"));
        }
        t.layout(&format!("theme_{name}"), &[]);
    }
    t.tap("Dark");
    t.row_panel("theme_accent_wheel", "Choose your own colour", 0);
    t.tap("Your own theme");
    t.layout("theme_own", &[]);
    for name in ["Background", "Cards", "Text"] {
        t.row_panel(&format!("theme_own_{name}"), name, 0);
    }
    t.tap("Dark");
}

/// Every row of the settings: a panel is gone through, a screen is looked at and left, a switch
/// goes over and back.
fn settings_walk(t: &Tour) {
    t.press("go", "2");
    t.screen("settings");
    let here = t.ui.get_screen();
    let els = t.els();
    let rows: Vec<(String, El)> = els
        .iter()
        .filter(|e| e.role == "Button" && e.w >= 300.0)
        .filter(|e| !e.t.contains("idget") && !e.t.starts_with("Rate "))
        .filter_map(|b| {
            els.iter()
                .filter(|e| {
                    e.role == "Text"
                        && (b.t == e.t
                            || b.t.starts_with(&format!("{} ", e.t))
                            || b.t.starts_with(&format!("{}, ", e.t)))
                })
                .max_by_key(|e| e.t.len())
                .map(|title| (title.t.clone(), b.clone()))
        })
        .collect();
    if rows.len() < 10 {
        t.problem(format!("only {} rows in the settings", rows.len()));
    }
    for (title, row) in rows {
        let name = format!("settings_{}", title.replace(' ', "_"));
        let (x, y) = (row.x, row.y);
        let open = move |t: &Tour| t.cmd(&format!("tapxy:{x} {y}")) == "OK";
        let under = t.baseline();
        open(t);
        if t.sheet_up() {
            t.choice_state.set(matches!(
                title.as_str(),
                "Language" | "Time format" | "Week starts on" | "Minimum volume"
            ));
            // Saving or restoring a backup would touch the phone's files.
            if title.starts_with("Back up") {
                t.opened(&name);
                t.layout(&name, &under);
                t.back();
            } else {
                t.panel(&name, &under, &|t: &Tour| open(t) && t.sheet_up());
            }
            t.choice_state.set(false);
        } else if t.ui.get_screen() != here {
            match title.as_str() {
                "Theme" => theme_walk(t),
                _ => t.screen(&name),
            }
            t.back();
        } else {
            t.problem(format!("{name}: the row opens nothing"));
        }
        if t.ui.get_screen() != here || t.sheet_up() {
            t.problem(format!("{name}: not back on the settings"));
            t.ui.set_panel_open(false);
            t.ui.set_screen(here);
        }
    }
    for sw in els.iter().filter(|e| e.role == "Switch") {
        t.tap(&sw.t);
        if t.is_on(&sw.t) == (sw.on == Some(true)) {
            t.problem(format!("the switch '{}' does not switch", sw.t));
        }
        t.tap(&sw.t);
        if t.is_on(&sw.t) != (sw.on == Some(true)) {
            t.problem(format!("the switch '{}' does not switch back", sw.t));
        }
    }
}

fn history_walk(t: &Tour) {
    t.press("go", "1");
    t.screen("history");
    for i in 0..7 {
        t.press("h-period", &i.to_string());
        if t.ui.get_h_period() != i {
            t.problem(format!("history period {i} does not take"));
        }
        t.layout(&format!("history_period_{i}"), &[]);
    }
    t.press("h-period", "0");
    if t.tapc("All stats") {
        t.layout("history_all_stats", &[]);
        t.tapc("Fewer stats");
    }
    let delete = |t: &Tour| {
        t.press("h-delete-history", "");
        t.sheet_up()
    };
    t.open_panel("history_delete", &delete);
    // A period asks first too, and Back is no.
    if delete(t) {
        t.tap("Older than 30 days");
        if t.has("Delete") {
            t.opened("history_delete_period");
            t.layout("history_delete_period", &[]);
            t.back();
            if t.sheet_up() {
                t.problem("history_delete_period: Back leaves it open");
                t.back();
            }
        } else {
            t.problem("deleting a period does not ask");
            t.back();
        }
    }
    t.press("go", "0");
}

fn add_history() {
    let now = chrono::Utc::now();
    for (i, days) in [0, 1, 2, 3, 4, 6, 8, 9, 5, 7].into_iter().enumerate() {
        let at = now - chrono::Duration::days(days);
        let event = crate::models::AlarmEvent {
            id: format!("tour{i}"),
            alarm_id: crate::models::AlarmId("work".into()),
            alarm_label: "Work".into(),
            fire_timestamp: at,
            first_interaction_timestamp: Some(at),
            dismiss_timestamp: at,
            snooze_count: (i % 3) as u8,
            snooze_intervals_minutes: vec![5; i % 3],
            challenge_solves: Vec::new(),
            total_dismissal_time: std::time::Duration::from_secs(40 + 25 * i as u64),
            outcome: match i {
                8 => crate::models::AlarmOutcome::Missed,
                9 => crate::models::AlarmOutcome::Limit,
                _ => crate::models::AlarmOutcome::Solved,
            },
        };
        crate::history_persistence::record_alarm_event(&event);
    }
}

/// The first start (the intro, then the setup) and a list with nothing in it.
fn first_start_walk(look: Look) -> Tour {
    let t = Tour::with_alarms(look, Vec::new());
    t.screen("home_empty");
    if !t.has("No alarms yet") {
        t.problem("no empty list");
    }
    t.press("go", "1");
    t.screen("history_empty");
    t.press("go", "0");
    feedback::open_intro(&t.ctx, true);
    if t.ui.get_screen() != SCREEN_INTRO {
        t.problem("no intro before the first-run screen");
    }
    t.screen("intro");
    t.tap("Skip");
    t.screen("setup");
    t
}

fn walk(t: &Tour) {
    t.screen("home");
    t.row_panel("home_pause", "Pause all alarms until a date", 0);
    // Pausing until a date has a calendar of its own.
    let under = t.baseline();
    if t.tapc("Pause all alarms until a date") && t.sheet_up() && t.tap("Until a date you pick") {
        t.opened("home_pause_date");
        t.layout("home_pause_date", &under);
        t.back();
        if t.sheet_up() {
            t.back();
        }
    } else {
        t.problem("no calendar for pausing until a date");
    }
    t.choice_state.set(true);
    t.row_panel("home_sort", "Sort alarms", 0);
    t.choice_state.set(false);
    for i in 0..3 {
        t.row_panel(&format!("home_menu{i}"), "More actions", i);
    }
    // A quick alarm says so, and Undo takes it back.
    t.tap("+5");
    if !t.ui.get_toast_visible() {
        t.problem("a quick alarm says nothing");
    }
    t.layout("home_quick_alarm", &[]);
    t.tap("Undo");
    editor_walk(t);
    history_walk(t);
    settings_walk(t);
}

/// What every walk has opened: a panel that no longer opens at all would otherwise just be missing.
const EXPECTED: [&str; 22] = [
    "home",
    "home_pause",
    "home_pause_date",
    "home_sort",
    "home_menu0",
    "editor_date",
    "editor_time",
    "editor_delete",
    "editor_Label",
    "editor_Sound",
    "editor_Vibration",
    "editor_Snooze",
    "editor_Alarm screen",
    "editor_Ring limit",
    "editor_Are you up?",
    "editor_Reminder",
    "editor_Challenges",
    "history_delete",
    "settings_Language",
    "theme_own_Background",
    "settings_Reliability",
    "intro",
];

/// The whole walk at one look: the first start and an empty list, then the list with everything on it.
fn tour(look: Look) {
    let t = first_start_walk(look);
    add_history();
    let t2 = Tour::new(look);
    walk(&t2);
    t.problems
        .borrow_mut()
        .extend(t2.problems.borrow().iter().cloned());
    t.opened
        .borrow_mut()
        .extend(t2.opened.borrow().iter().cloned());
    let opened = t.opened.borrow();
    for name in EXPECTED {
        if !opened.iter().any(|o| o == name) {
            t.problem(format!("{name} was never opened"));
        }
    }
    if std::env::var_os("TOUR_TRACE").is_some() {
        println!("{} opened {:?}", look.name, opened);
    }
    let problems = t.problems.borrow();
    assert!(
        problems.is_empty(),
        "{} problems:\n{}",
        problems.len(),
        problems.join("\n")
    );
}

#[test]
fn every_panel_on_a_phone() {
    tour(LOOKS[0]);
}

#[test]
fn every_panel_on_a_phone_with_big_text() {
    tour(LOOKS[1]);
}

#[test]
fn every_panel_at_560_dpi() {
    tour(LOOKS[2]);
}

#[test]
fn every_panel_at_640_dpi() {
    tour(LOOKS[3]);
}
