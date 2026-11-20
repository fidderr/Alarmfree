//! Scenarios of the device suite (devtest/matrix.sh) that only look at what the window shows and
//! does, run in `cargo test` on Slint's testing backend: a person's touches through the accessible
//! labels (`debug_elements`), the same callbacks, the same saved state. A scenario that needs a
//! real phone for part of it keeps that part on the device; the table old -> new is in
//! docs/test-moves.md.

use std::cell::RefCell;
use std::rc::Rc;

use super::a11y_tests::alarm;
use super::*;
use crate::models::TimeFormat;

struct Flow {
    ui: MainWindow,
    ctx: Rc<Ctx>,
    problems: RefCell<Vec<String>>,
}

impl Flow {
    fn new(alarms: Vec<AlarmData>) -> Flow {
        Flow::with_settings(AppSettings::default(), alarms)
    }

    fn with_settings(settings: AppSettings, alarms: Vec<AlarmData>) -> Flow {
        Flow::with_look(settings, alarms, 3.0, 1.0)
    }

    /// The density and text size of the large sizes the owner's testers had (s65), or a phone's.
    fn with_look(mut settings: AppSettings, alarms: Vec<AlarmData>, scale: f32, font: f32) -> Flow {
        thread_local!(static INIT: () = i_slint_backend_testing::init_no_event_loop());
        INIT.with(|_| {});
        settings.prefs.less_motion = true;
        settings.prefs.setup_done = true;
        // The clock stands still on a Wednesday at ten: no flow depends on the weekday or on the
        // minute the test happens to run in.
        crate::clock::set_fixed(Some(wednesday_ten()));
        crate::persistence::save_alarms(&alarms);
        let ui = MainWindow::new().unwrap();
        // The size of the emulator suite's phone, tall enough that nothing needs a scroll.
        ui.window()
            .dispatch_event(slint::platform::WindowEvent::ScaleFactorChanged {
                scale_factor: scale,
            });
        ui.window().set_size(slint::PhysicalSize::new(1080, 6000));
        ui.global::<Motion>().set_less(true);
        ui.global::<Zoom>().set_scale(font);
        let ctx = new_ctx(&ui, settings, alarms);
        let weak = Rc::downgrade(&ctx);
        ui.global::<Tr>().on_lookup(move |key, _| {
            weak.upgrade()
                .map(|ctx| i18n::t(&ctx.tr.borrow(), key.as_str()))
                .unwrap_or_else(|| key.to_string())
                .into()
        });
        connect_layout(&ui);
        apply_theme(&ctx);
        connect(&ctx, &ui);
        ui.show().unwrap();
        home::refresh(&ctx);
        let flow = Flow {
            ui,
            ctx,
            problems: RefCell::new(Vec::new()),
        };
        flow.settle();
        flow
    }

    fn settle(&self) {
        i_slint_backend_testing::mock_elapsed_time(std::time::Duration::from_millis(800));
    }

    fn cmd(&self, command: &str) -> String {
        let said = debug_elements::run(&self.ui, command).unwrap();
        if command.starts_with("tap") {
            self.settle();
        }
        said
    }

    /// Touch the element that says exactly this (`tapc`: has it in it); false when none does.
    fn tap(&self, label: &str) -> bool {
        self.cmd(&format!("tap:{label}")) == "OK"
    }
    fn tap_has(&self, label: &str) -> bool {
        self.cmd(&format!("tapc:{label}")) == "OK"
    }
    fn tap_last(&self, label: &str) -> bool {
        self.cmd(&format!("tap@L:{label}")) == "OK"
    }
    fn has(&self, label: &str) -> bool {
        self.cmd(&format!("find:{label}")) != "none"
    }
    fn has_part(&self, label: &str) -> bool {
        self.cmd(&format!("findc:{label}")) != "none"
    }
    fn texts(&self) -> Vec<String> {
        let rows: Vec<serde_json::Value> = serde_json::from_str(&self.cmd("texts")).unwrap();
        rows.iter()
            .map(|r| r["t"].as_str().unwrap().to_owned())
            .collect()
    }
    fn press(&self, name: &str, arg: &str) {
        let said = debug::run(&self.ctx, &format!("press:{name}:{arg}"));
        assert_eq!(said, "OK", "press {name}");
        self.settle();
    }
    fn back(&self) {
        super::back(&self.ctx);
        self.settle();
    }
    /// A property of the window from the debug snapshot, as the device scenarios read it (`sn`).
    fn sn(&self, key: &str) -> serde_json::Value {
        let snapshot: serde_json::Value =
            serde_json::from_str(&debug::run(&self.ctx, "snapshot")).unwrap();
        snapshot[key].clone()
    }
    fn screen(&self) -> i32 {
        self.ui.get_screen()
    }
    fn sheet_open(&self) -> bool {
        self.ui.get_home_sheet() != HomeSheet::None
    }
    fn editor_sheet_open(&self) -> bool {
        self.ui.get_ed_sheet() != EditorSheet::None
    }
    fn panel_open(&self) -> bool {
        self.ui.get_panel_open()
    }

    fn problem(&self, msg: impl std::fmt::Display) {
        self.problems.borrow_mut().push(msg.to_string());
    }
    fn expect(&self, ok: bool, msg: impl std::fmt::Display) {
        if !ok {
            self.problem(msg);
        }
    }
    fn finish(&self) {
        let problems = self.problems.borrow();
        assert!(problems.is_empty(), "{}", problems.join("; "));
    }
}

/// 2026-11-18 10:00 in the phone's time zone, in unix ms.
fn wednesday_ten() -> i64 {
    use chrono::TimeZone;
    chrono::Local
        .with_ymd_and_hms(2026, 11, 18, 10, 0, 0)
        .unwrap()
        .timestamp_millis()
}

fn daily(id: &str, hour: u8, minute: u8, label: &str) -> AlarmData {
    let mut a = alarm(id, hour, label);
    a.time_minute = minute;
    a.days = [true; 7];
    a
}

/// s25: Back closes a sheet first, then the screen; an editor with changes asks before it goes;
/// the main screens do not stack up.
#[test]
fn s25_back_navigation() {
    let f = Flow::new(vec![daily("a", 7, 0, "Work")]);
    f.expect(f.screen() == 0, "not on Home");
    // A sheet closes first, and the screen stays.
    f.expect(f.tap("More actions"), "no menu button");
    f.expect(f.sheet_open(), "no menu");
    f.back();
    f.expect(!f.sheet_open(), "Back left the menu open");
    f.expect(f.screen() == 0, "Back from the menu left Home");
    // Then the screen.
    f.expect(f.tap_has("07:00, "), "no card to open");
    f.expect(f.screen() == 1, "no editor");
    f.expect(f.tap_has("Label"), "no label row");
    f.expect(f.editor_sheet_open(), "no editor sheet");
    f.back();
    f.expect(!f.editor_sheet_open(), "Back left the editor sheet open");
    f.expect(f.screen() == 1, "Back from the sheet left the editor");
    f.back();
    f.expect(f.screen() == 0, "Back from the editor did not reach Home");
    // With changes it asks first.
    f.expect(f.tap_has("07:00, "), "no card to open the second time");
    f.expect(f.tap("+1"), "no +1");
    f.back();
    f.expect(f.editor_sheet_open(), "no question about discarding");
    f.expect(
        f.has("Keep editing") && f.has("Discard"),
        format!("the question has no buttons: {:?}", f.texts()),
    );
    f.back();
    f.expect(!f.editor_sheet_open(), "Back from the question stayed");
    f.expect(f.screen() == 1, "Back from the question left the editor");
    f.back();
    f.expect(f.editor_sheet_open(), "no question the second time");
    f.expect(f.tap("Discard"), "no Discard");
    f.expect(f.screen() == 0, "discarding did not reach Home");
    // The main screens do not stack up.
    f.expect(f.tap_last("History"), "no History tab");
    f.expect(f.screen() == 2, "no history");
    f.expect(f.tap_last("Settings"), "no Settings tab");
    f.expect(f.screen() == 3, "no settings");
    f.back();
    f.expect(f.screen() == 0, "Back from settings did not reach Home");
    f.finish();
}

/// s46: a 12-hour clock in the list, the editor and the wheels.
#[test]
fn s46_twelve_hours() {
    let mut settings = AppSettings::default();
    settings.prefs.time_format = TimeFormat::H12;
    let f = Flow::with_settings(settings, vec![daily("a", 14, 5, "Work")]);
    f.expect(
        f.has_part("2:05 PM"),
        format!("the card does not say 2:05 PM: {:?}", f.texts()),
    );
    f.press("open", "a");
    f.expect(f.screen() == 1, "no editor");
    f.expect(
        f.sn("ed_time_text") == "2:05 PM",
        format!("editor says {}", f.sn("ed_time_text")),
    );
    f.expect(f.tap_has("Time:"), "no time button");
    let wheels = f.texts().join("|");
    f.expect(
        wheels.contains("PM"),
        format!("the wheels do not say PM: {wheels}"),
    );
    f.finish();
}

fn event(id: &str, days_ago: i64) -> crate::models::AlarmEvent {
    let at = crate::clock::utc_now() - chrono::Duration::days(days_ago);
    crate::models::AlarmEvent {
        id: id.into(),
        alarm_id: crate::models::AlarmId("a".into()),
        alarm_label: "a".into(),
        fire_timestamp: at,
        first_interaction_timestamp: None,
        dismiss_timestamp: at,
        snooze_count: 0,
        snooze_intervals_minutes: Vec::new(),
        challenge_solves: Vec::new(),
        total_dismissal_time: std::time::Duration::ZERO,
        outcome: Default::default(),
    }
}

fn history_count() -> usize {
    crate::history_persistence::query_history(&crate::models::HistoryFilter::LastDays(100_000))
        .len()
}

/// s43: a period of the history and all of it each go only after asking again, and Back is no.
#[test]
fn s43_delete_history() {
    crate::clock::set_fixed(Some(wednesday_ten()));
    crate::history_persistence::record_alarm_event(&event("old", 200));
    crate::history_persistence::record_alarm_event(&event("new", 1));
    let f = Flow::new(vec![daily("a", 7, 0, "a")]);
    f.expect(f.tap_last("History"), "no History tab");
    f.expect(f.screen() == 2, "no history");
    // A period asks first: Back is no, and the answer takes only that period.
    f.press("h-delete-history", "");
    f.expect(f.panel_open(), "no sheet");
    f.expect(f.tap("Older than 30 days"), "no 'Older than 30 days'");
    f.expect(f.has("Delete"), "older than 30 days went without asking");
    f.back();
    f.expect(!f.panel_open(), "Back left the period question open");
    f.expect(history_count() == 2, "Back on the period question deleted");
    f.press("h-delete-history", "");
    f.tap("Older than 30 days");
    f.expect(f.tap("Delete"), "no Delete");
    f.expect(
        history_count() == 1,
        format!("older than 30 days left {}", history_count()),
    );
    f.expect(f.sn("toast_visible") == true, "nothing said about it");
    f.press("h-delete-history", "");
    f.expect(f.tap("Everything"), "no Everything");
    f.expect(history_count() == 1, "everything went without asking again");
    f.back();
    f.expect(!f.panel_open(), "Back left the question open");
    f.expect(history_count() == 1, "Back deleted it");
    f.press("h-delete-history", "");
    f.tap("Everything");
    f.tap("Delete");
    f.expect(
        history_count() == 0,
        format!("left {} after confirming", history_count()),
    );
    f.finish();
}

fn saved(id: &str) -> AlarmData {
    crate::persistence::load_alarms()
        .into_iter()
        .find(|a| a.id == id)
        .unwrap_or_else(|| panic!("alarm {id} is not saved"))
}

/// s30: a nap in two taps, and Undo.
#[test]
fn s30_quick_alarm_and_undo() {
    let f = Flow::new(Vec::new());
    f.expect(f.tap("+5"), format!("no +5: {:?}", f.texts()));
    let made = crate::persistence::load_alarms()
        .iter()
        .filter(|a| a.delete_after_ring)
        .count();
    f.expect(made == 1, format!("made {made}"));
    f.expect(f.sn("toast_visible") == true, "nothing said about it");
    f.expect(f.tap("Undo"), "no Undo");
    let left = crate::persistence::load_alarms()
        .iter()
        .filter(|a| a.delete_after_ring)
        .count();
    f.expect(left == 0, format!("left {left} after Undo"));
    f.finish();
}

/// s31: pausing everything moves the pause past a week, and Undo brings it back.
#[test]
fn s31_pause_all() {
    let f = Flow::new(vec![daily("a", 7, 0, "a")]);
    f.expect(f.tap("Pause all alarms until a date"), "no pause button");
    f.expect(
        f.tap("For a week"),
        format!("no 'For a week': {:?}", f.texts()),
    );
    let until = saved("a").pause_until_ms;
    let week = 6 * 24 * 3600 * 1000;
    f.expect(
        until.is_some_and(|u| u >= wednesday_ten() + week),
        format!("paused until {until:?}"),
    );
    f.expect(f.tap("Undo"), "no Undo");
    f.expect(saved("a").pause_until_ms.is_none(), "Undo left it paused");
    f.finish();
}

/// s47: from an alarm's menu, new alarms start like this one, without its name.
#[test]
fn s47_use_for_new_alarms() {
    let mut a = daily("a", 7, 0, "a");
    a.note = "Keep out".into();
    a.sound = "lofi".into();
    a.fade_in_secs = 60;
    a.snooze_count = 2;
    a.snooze_interval = 9;
    let f = Flow::new(vec![a]);
    f.expect(f.tap("More actions"), "no menu");
    f.expect(f.tap("Use for new alarms"), "no 'Use for new alarms'");
    f.expect(f.sn("toast_visible") == true, "nothing said about it");
    f.expect(f.tap("Create new alarm"), "no create button");
    f.expect(f.screen() == 1, "no editor");
    let rows = f.texts().join("|");
    f.expect(
        rows.contains("Lofi"),
        format!("sound not taken over: {rows}"),
    );
    f.expect(
        rows.contains("9 min, 2"),
        format!("snooze not taken over: {rows}"),
    );
    f.expect(
        !rows.contains("Keep out") && !rows.contains("Label a"),
        format!("its name came along: {rows}"),
    );
    f.finish();
}

/// s65: at a large display size and text the alarm card, and the editor's rows, stay inside the window
/// and inside their card.
#[test]
fn s65_large_size_fits() {
    for (scale, font) in [(3.5, 1.3), (4.0, 1.15)] {
        let f = Flow::with_look(
            AppSettings::default(),
            vec![daily("a", 7, 0, "a")],
            scale,
            font,
        );
        let said = f.cmd("fit:AlarmCard");
        f.expect(said == "none", format!("home at {scale}/{font}: {said}"));
        f.press("open", "a");
        f.expect(f.screen() == 1, "no editor");
        let said = f.cmd("fit:RowList");
        f.expect(said == "none", format!("editor at {scale}/{font}: {said}"));
        f.finish();
    }
}

/// s61 (the part without a finger on the wheel): a theme that comes with the app, a colour in a
/// code, a theme of one's own, and text that is made readable.
#[test]
fn s61_colours_of_ones_own() {
    let f = Flow::new(vec![daily("a", 7, 0, "a")]);
    f.expect(f.tap_last("Settings"), "no Settings tab");
    f.expect(f.tap_has("Theme"), "no Theme row");
    f.expect(f.ui.get_screen() == SCREEN_THEME, "no theme screen");
    let names = f.texts();
    for theme in [
        "Dark",
        "Light",
        "Pure black",
        "Midnight",
        "Forest",
        "Plum",
        "Sand",
        "Mist",
    ] {
        f.expect(
            names.iter().any(|t| t == theme),
            format!("no theme {theme}"),
        );
    }
    f.expect(f.tap("Sand"), "no Sand");
    f.expect(
        f.sn("pal_bg") == "#f3ede2",
        format!("Sand is {}", f.sn("pal_bg")),
    );
    f.expect(f.sn("pal_light") == true, "Sand is not light");
    f.finish();
}

fn row_labels(f: &Flow) -> String {
    f.sn("rows")
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["label"].as_str().unwrap().to_owned())
        .collect::<Vec<_>>()
        .join(" ")
}

/// s52 (without the pause calendar and the skip, which need the OS's next ring): sort, duplicate and
/// the switch.
#[test]
fn s52_alarm_menu() {
    let mut a = daily("a", 9, 0, "a");
    a.order = 1;
    let mut b = daily("b", 7, 0, "b");
    b.order = 2;
    let f = Flow::new(vec![a, b]);
    f.expect(
        row_labels(&f) == "a b",
        format!("own order: {}", row_labels(&f)),
    );
    f.expect(f.tap("Sort alarms") && f.tap("By time"), "no sort menu");
    f.expect(
        row_labels(&f) == "b a",
        format!("by time: {}", row_labels(&f)),
    );
    f.expect(
        f.tap("Sort alarms") && f.tap("Your own order"),
        "no own order",
    );
    f.expect(
        row_labels(&f) == "a b",
        format!("back to own: {}", row_labels(&f)),
    );
    // Duplicate: an editor with the copy, that saves as one more.
    f.expect(f.tap("More actions"), "no menu for duplicate");
    f.expect(f.tap("Duplicate"), "no Duplicate");
    f.expect(f.screen() == 1, "no editor for the copy");
    f.expect(
        f.has_part("a (copy)"),
        format!("no copy name: {:?}", f.texts()),
    );
    f.expect(f.tap("Save"), "no Save");
    f.expect(
        crate::persistence::load_alarms().len() == 3,
        format!(
            "{} alarms after duplicating",
            crate::persistence::load_alarms().len()
        ),
    );
    // The switch.
    f.expect(f.tap("On or off: a"), "no switch");
    f.expect(!saved("a").enabled, "the switch left it on");
    f.expect(f.tap("On or off: a"), "no switch again");
    f.expect(saved("a").enabled, "not back on");
    f.finish();
}

impl Flow {
    fn set_field(&self, n: usize, text: &str) {
        let said = self.cmd(&format!("settext:{n}:{text}"));
        assert_eq!(said, "OK", "field {n}");
        self.settle();
    }
    fn done(&self) {
        assert!(self.tap_last("Done"), "no Done: {:?}", self.texts());
    }
    fn open_row(&self, row: &str) {
        assert!(self.tap_has(row), "no row {row}: {:?}", self.texts());
    }
}

/// s50 (the settings that need no scroll, slider or bundled sound): a new alarm made through the screens, what is
/// saved, what the editor says when it is opened again, and deleting it after asking.
#[test]
fn s50_editor_saves_it_all() {
    let f = Flow::new(Vec::new());
    f.expect(f.tap("Create new alarm"), "no create button");
    f.expect(f.screen() == 1, "no editor");
    f.expect(f.tap("+5") && f.tap("+5"), "no +5");
    f.expect(f.tap("Weekend"), "no Weekend");
    f.open_row("Label");
    f.set_field(0, "Morning");
    f.set_field(1, "Take pills");
    f.done();
    f.open_row("Vibration");
    f.expect(f.tap("Steady"), "no Steady");
    f.done();
    f.expect(f.tap("Full brightness"), "no Full brightness");
    f.expect(f.tap("Save"), "no Save");
    f.expect(f.screen() == 0, "Save did not go back");
    let all = crate::persistence::load_alarms();
    f.expect(all.len() == 1, format!("{} alarms saved", all.len()));
    let a = all.first().cloned().unwrap_or_else(|| alarm("x", 0, ""));
    f.expect(
        a.time_hour == 7 && a.time_minute == 10,
        format!("time {}:{}", a.time_hour, a.time_minute),
    );
    f.expect(
        a.label == "Morning" && a.note == "Take pills",
        format!("label {:?} note {:?}", a.label, a.note),
    );
    f.expect(
        a.days == [false, false, false, false, false, true, true],
        format!("days {:?}", a.days),
    );
    f.expect(a.vibration == 1, format!("vibration {}", a.vibration));
    f.expect(a.max_brightness, "full brightness not saved");
    // What it says when opened again.
    f.press("open", &a.id);
    let shown = f.texts().join("|");
    for want in ["Morning", "Steady"] {
        f.expect(
            shown.contains(want),
            format!("no {want} when opened again: {shown}"),
        );
    }
    f.expect(
        f.sn("ed_time_text")
            .as_str()
            .is_some_and(|t| t.ends_with(":10")),
        "time not shown",
    );
    f.finish();
}

/// s51 (until the restart): every setting through its own screen is saved.
#[test]
fn s51_settings_stick() {
    let f = Flow::new(vec![daily("a", 14, 5, "a")]);
    f.expect(f.tap_last("Settings"), "no Settings tab");
    f.expect(f.screen() == 3, "no settings");
    let pick = |row: &str, choice: &str| {
        f.expect(f.tap_has(row), format!("no row {row}: {:?}", f.texts()));
        f.expect(f.panel_open(), format!("{row} opens no panel"));
        f.expect(
            f.tap(choice),
            format!("no choice {choice}: {:?}", f.texts()),
        );
        if f.panel_open() {
            f.back();
        }
    };
    pick("Time format", "12 hours");
    pick("Week starts on", "Sunday");
    f.expect(f.tap_has("Minimum volume"), "no minimum volume row");
    f.expect(f.tap("75%"), "no 75%");
    f.expect(f.tap_last("Done"), "no Done");
    f.expect(f.tap("Touch feedback"), "no Touch feedback");
    f.expect(f.tap("Less motion"), "no Less motion");
    f.expect(f.tap("Confetti"), "no Confetti");
    let saved = crate::persistence::load_settings();
    f.expect(
        saved.prefs.time_format == TimeFormat::H12,
        "12 hours not saved",
    );
    f.expect(
        saved.min_volume_percent == 75,
        format!("volume {}", saved.min_volume_percent),
    );
    f.expect(!saved.prefs.touch_feedback, "touch feedback still on");
    // The flows start with less motion on (no animations): the switch turns it off.
    f.expect(!saved.prefs.less_motion, "less motion still on");
    f.expect(!saved.prefs.confetti, "confetti still on");
    f.expect(
        format!("{:?}", saved.prefs.week_start) == "Sunday",
        format!("week starts on {:?}", saved.prefs.week_start),
    );
    f.finish();
}

/// s53 (the part without a phone's own screens): a template makes an alarm, a typed time, a tap on
/// the wheel, one date, a challenge with a setting and one taken away, saved as set.
#[test]
fn s53_the_rest_of_the_screens() {
    let f = Flow::new(Vec::new());
    f.expect(f.has("No alarms yet"), "no empty state");
    f.expect(f.tap_has("Weekdays at"), "no template");
    f.expect(f.sn("toast_visible") == true, "a template says nothing");
    f.expect(
        crate::persistence::load_alarms().len() == 1,
        "a template makes no alarm",
    );
    let id = crate::persistence::load_alarms()[0].id.clone();
    f.press("open", &id);
    f.expect(f.screen() == 1, "the alarm from a template opens no editor");
    // Typed, then a tap on the wheel.
    f.expect(f.tap_has("Time:"), "no time button");
    f.expect(f.tap("Type it"), "no Type it");
    f.set_field(0, "835");
    f.done();
    f.expect(
        f.sn("ed_time_text")
            .as_str()
            .is_some_and(|t| t.ends_with(":35")),
        format!("typed time shows {}", f.sn("ed_time_text")),
    );
    f.expect(f.tap_has("Time:"), "no time button again");
    f.press("ed-minute", "36");
    f.done();
    f.expect(
        f.sn("ed_time_text")
            .as_str()
            .is_some_and(|t| t.ends_with(":36")),
        format!("wheel time shows {}", f.sn("ed_time_text")),
    );
    // One date.
    f.expect(f.tap("One date"), "no One date");
    f.expect(f.tap("Tomorrow"), "no Tomorrow");
    f.done();
    // A challenge with a setting, and one taken away again.
    f.open_row("Challenges");
    f.expect(f.tap("Add a challenge"), "no add");
    f.expect(f.tap_last("Math"), "no Math");
    if f.has("Hard") {
        f.tap("Hard");
    }
    if f.has("Done") {
        f.done();
    }
    f.expect(f.tap("Add a challenge"), "no add again");
    f.expect(f.tap_last("Shake"), "no Shake");
    if f.has("Done") {
        f.done();
    }
    let shown = f.texts();
    f.expect(
        shown
            .iter()
            .filter(|t| *t == "Math" || *t == "Shake")
            .count()
            >= 2,
        format!("added: {shown:?}"),
    );
    f.press("challenge-remove", "1");
    f.expect(
        !f.texts().iter().any(|t| t == "Shake"),
        "Shake not taken away",
    );
    f.done();
    f.expect(f.tap("Save"), "no Save");
    let a = crate::persistence::load_alarms()[0].clone();
    f.expect(a.time_minute == 36, format!("minute {}", a.time_minute));
    f.expect(a.days == [false; 7], format!("days {:?}", a.days));
    f.expect(
        a.date.as_deref() == Some("2026-11-19"),
        format!("date {:?}", a.date),
    );
    let json = serde_json::to_string(&a.challenges).unwrap();
    f.expect(
        json.contains("challenge.math") && !json.contains("challenge.shake"),
        format!("challenges {json}"),
    );
    f.finish();
}

/// The squat challenge: in the picker, its settings, tried out with squats counted by the sensor
/// (injected), and saved with the alarm.
#[test]
fn squat_challenge_from_picker_to_try_out() {
    let f = Flow::new(Vec::new());
    f.expect(f.tap("Create new alarm"), "no create button");
    f.open_row("Challenges");
    f.expect(f.tap("Add a challenge"), "no add");
    f.expect(
        f.has("Squat") && f.has("Do squats"),
        format!("the picker has no Squat: {:?}", f.texts()),
    );
    f.expect(f.tap_last("Squat"), "no Squat");
    f.expect(
        f.has("How many squats") && f.has("10"),
        format!("the settings of Squat: {:?}", f.texts()),
    );
    f.expect(f.tap_has("More How many squats"), "no increase");
    f.expect(f.has("15"), "15 after one step up");
    // Tried: the counter screen, then the counts the sensor reports.
    super::challenge_run::inject_counts([0, 0, 0]);
    f.expect(f.tap("Try"), "no Try");
    let run = f.ui.global::<Run>();
    f.expect(run.get_active(), "the try-out did not start");
    f.expect(
        run.get_ring_text() == "0 / 15",
        format!("ring says {}", run.get_ring_text()),
    );
    f.expect(
        f.has("Hold your phone steady and squat!"),
        format!("instruction: {:?}", f.texts()),
    );
    super::challenge_run::inject_counts([0, 0, 7]);
    f.settle();
    f.expect(
        run.get_ring_text() == "7 / 15",
        format!("ring says {} at 7", run.get_ring_text()),
    );
    f.expect(!run.get_celebrating(), "done at 7 of 15");
    super::challenge_run::inject_counts([0, 0, 15]);
    f.settle();
    f.expect(run.get_celebrating(), "15 of 15 is not celebrated");
    i_slint_backend_testing::mock_elapsed_time(std::time::Duration::from_millis(2500));
    f.expect(
        f.has("That is how it will go. Nothing was recorded."),
        format!("no end of the try-out: {:?}", f.texts()),
    );
    super::challenge_run::inject_counts([0; 3]);
    f.tap("Exit");
    f.expect(!run.get_active(), "Exit leaves the try-out up");
    // Saved with the alarm.
    for _ in 0..2 {
        if f.has("Done") {
            f.done();
        }
    }
    f.expect(f.tap("Save"), "no Save");
    let stored = crate::persistence::load_alarms()[0].challenges.clone();
    f.expect(
        stored == vec![("challenge.squat".to_owned(), r#"{"count":15}"#.to_owned())],
        format!("stored {stored:?}"),
    );
    f.finish();
}

fn draft_reminder(f: &Flow) -> crate::models::Reminder {
    f.ctx
        .editor
        .borrow()
        .as_ref()
        .map(|ed| ed.draft.reminder.clone())
        .unwrap_or_default()
}

/// The reminder sheet: off to begin with, every time step picks its minutes, the text and the
/// sound only show once it is on, Off keeps what was typed, and Save keeps it all.
#[test]
fn s67_reminder_sheet() {
    let f = Flow::new(Vec::new());
    f.expect(f.tap("Create new alarm"), "no create button");
    f.expect(
        f.has_part("Reminder") && f.has_part("Off"),
        "the editor has no Reminder row saying Off",
    );
    f.open_row("Reminder");
    f.expect(
        !f.has_part("Your text") && !f.has_part("Notification sound"),
        "text and sound show while the reminder is off",
    );
    let steps: [(&str, u16); 19] = [
        ("5 min", 5),
        ("10 min", 10),
        ("15 min", 15),
        ("30 min", 30),
        ("45 min", 45),
        ("1 h", 60),
        ("2 h", 120),
        ("3 h", 180),
        ("4 h", 240),
        ("5 h", 300),
        ("6 h", 360),
        ("7 h", 420),
        ("8 h", 480),
        ("9 h", 540),
        ("10 h", 600),
        ("11 h", 660),
        ("12 h", 720),
        ("16 h", 960),
        ("24 h", 1440),
    ];
    for (label, minutes) in steps {
        f.expect(f.tap_last(label), format!("no step {label}"));
        let r = draft_reminder(&f);
        f.expect(r.on && r.minutes == minutes, format!("{label} gave {r:?}"));
    }
    f.expect(
        f.has_part("Your text") && f.has_part("Notification sound"),
        "text and sound do not show with the reminder on",
    );
    f.expect(!draft_reminder(&f).sound, "a reminder starts silent");
    // Text and sound.
    f.set_field(0, "Go to bed, your alarm goes off in 8 hours");
    f.expect(f.tap_last("8 h"), "no 8 h");
    f.expect(f.tap_last("Notification sound"), "no sound choice");
    let r = draft_reminder(&f);
    f.expect(
        r.sound && r.text == "Go to bed, your alarm goes off in 8 hours" && r.minutes == 480,
        format!("text and sound gave {r:?}"),
    );
    f.expect(f.tap_last("Silent"), "no silent choice");
    f.expect(!draft_reminder(&f).sound, "Silent did not take it back");
    f.expect(f.tap_last("Notification sound"), "no sound choice again");
    // Off keeps what was typed, and turns it off.
    f.expect(f.tap_last("Off"), "no Off");
    let r = draft_reminder(&f);
    f.expect(
        !r.on && r.text.starts_with("Go to bed") && r.sound,
        format!("Off gave {r:?}"),
    );
    f.expect(!f.has_part("Your text"), "text shows with the reminder off");
    f.expect(f.tap_last("15 min"), "no 15 min");
    // A text too long for a reminder is cut.
    f.set_field(0, &"x".repeat(130));
    f.expect(
        draft_reminder(&f).text.chars().count() == 120,
        format!(
            "the long text is {} long",
            draft_reminder(&f).text.chars().count()
        ),
    );
    f.expect(
        f.cmd("set:0:Dentist") == "OK",
        "no field to replace the text in",
    );
    f.settle();
    f.done();
    f.expect(
        f.has("15 min before, with sound"),
        format!("the summary line: {:?}", f.texts()),
    );
    f.expect(f.tap("Save"), "no Save");
    let a = crate::persistence::load_alarms()[0].clone();
    f.expect(
        a.reminder
            == crate::models::Reminder {
                on: true,
                minutes: 15,
                text: "Dentist".to_owned(),
                sound: true,
            },
        format!("saved {:?}", a.reminder),
    );
    // Opened again it says the same.
    f.press("open", &a.id);
    f.expect(
        f.has("15 min before, with sound"),
        "the summary is gone after saving",
    );
    f.open_row("Reminder");
    f.expect(
        draft_reminder(&f).text == "Dentist" && f.has_part("Your text"),
        "the text is gone after saving",
    );
    f.finish();
}
