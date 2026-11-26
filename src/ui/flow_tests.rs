//! Scenarios of the device suite (devtest/matrix.sh) that only look at what the window shows and
//! does, run in `cargo test` on Slint's testing backend: a person's touches through the accessible
//! labels (`debug_elements`), the same callbacks, the same saved state. A scenario that needs a
//! real phone for part of it keeps that part on the device; the table old -> new is in
//! docs/test-moves.md.

use std::cell::RefCell;
use std::collections::BTreeSet;
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
    /// One typing, then what that field holds (`heldat`), as `type_field` of devtest/lib.sh does it.
    fn type_into(&self, n: usize, text: &str) -> String {
        let said = self.cmd(&format!("settext:{n}:{text}"));
        let place = said
            .strip_prefix("OK ")
            .unwrap_or_else(|| panic!("field {n}: {said}"))
            .replace(' ', ":");
        self.settle();
        self.cmd(&format!("heldat:{place}"))
    }
    fn set_field(&self, n: usize, text: &str) {
        let held = self.type_into(n, text);
        assert_eq!(held, format!("TEXT:{text}"), "field {n}");
    }
    fn done(&self) {
        assert!(self.tap_last("Done"), "no Done: {:?}", self.texts());
    }
    fn open_row(&self, row: &str) {
        assert!(self.tap_has(row), "no row {row}: {:?}", self.texts());
    }
}

/// The field of a sound being renamed has the focus by itself. `settext:focus:` types into it with no touch: a touch on the place the
/// field was read at can miss it when the sheet has moved since (the keyboard rising, on a slow phone), and nothing is typed (s38 on
/// Android 16, 2026-11-26). Without a focused field it says so and types nothing.
#[test]
fn typing_into_the_focused_field_makes_no_touch() {
    struct Sound(std::path::PathBuf);
    impl Drop for Sound {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }
    let touches = |f: &Flow| f.cmd("trace").matches("tap_at").count();
    let dir = crate::custom_sounds::custom_sounds_dir();
    std::fs::create_dir_all(&dir).unwrap();
    let _sound = Sound(dir.join("mytune.ogg"));
    std::fs::write(&_sound.0, b"OggS").unwrap();
    let f = Flow::new(Vec::new());
    f.expect(f.tap("Create new alarm"), "no create button");
    f.open_row("Sound");
    f.expect(
        f.cmd("settext:focus:x") == "none",
        "a field was typed into with none focused",
    );
    let own = super::editor_sound::sound_ids().len() - 1;
    f.press("sound-rename", &own.to_string());
    f.settle();
    let focused = f.cmd("cursor");
    f.expect(
        focused != "none",
        format!("the rename field has no focus: {focused}"),
    );
    let before = touches(&f);
    let said = f.cmd("settext:focus:Morning");
    f.settle();
    f.expect(
        said.starts_with("OK "),
        format!("settext:focus said {said}"),
    );
    f.expect(
        f.cmd("cursor").contains("\"t\":\"Morning\""),
        format!("the field holds {}", f.cmd("cursor")),
    );
    f.expect(
        touches(&f) == before,
        format!(
            "typing into the focused field made a touch: {}",
            f.cmd("trace")
        ),
    );
    f.expect(
        f.editor_sheet_open(),
        "the sheet closed while a name was typed",
    );
    f.press("sound-rename-done", "");
    f.settle();
    f.finish();
}

/// The second field of a sheet is reached with the Tab key, not with a touch on the place it was read at (the sheet moves while the keyboard
/// rises). Nothing is typed when Tab does not move the focus.
#[test]
fn the_next_field_is_reached_with_tab_and_no_touch() {
    let touches = |f: &Flow| f.cmd("trace").matches("tap_at").count();
    let f = Flow::new(Vec::new());
    f.expect(f.tap("Create new alarm"), "no create button");
    f.open_row("Label");
    f.set_field(0, "Morning");
    let before = touches(&f);
    let said = f.cmd("settext:next:Take pills");
    f.settle();
    f.expect(said.starts_with("OK "), format!("settext:next said {said}"));
    let fields = f.cmd("inputs");
    f.expect(
        fields.contains("\"t\":\"Morning\"") && fields.contains("\"t\":\"Take pills\""),
        format!("the fields hold {fields}"),
    );
    f.expect(touches(&f) == before, "Tab was a touch");
    // The last field has no next one in the sheet: nothing is typed anywhere.
    f.done();
    f.finish();
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

/// Haptics of a counted challenge: one soft tick per newly counted squat, and the beaten pattern
/// (not the soft tick) when the count is reached.
#[test]
fn squat_haptics_tick_and_beaten_pattern() {
    use crate::feedback::{self, Haptic};
    let f = Flow::new(Vec::new());
    f.expect(f.tap("Create new alarm"), "no create button");
    f.open_row("Challenges");
    f.expect(f.tap("Add a challenge"), "no add");
    f.expect(f.tap_last("Squat"), "no Squat");
    super::challenge_run::inject_counts([0, 0, 0]);
    f.expect(f.tap("Try"), "no Try");
    let soft = Haptic::Waveform(
        feedback::SOFT_TICK_TIMINGS.to_vec(),
        feedback::SOFT_TICK_AMPLITUDES.to_vec(),
    );
    let beaten = Haptic::Waveform(
        feedback::BEATEN_TIMINGS.to_vec(),
        feedback::BEATEN_AMPLITUDES.to_vec(),
    );
    feedback::take_recorded();
    super::challenge_run::inject_counts([0, 0, 1]);
    f.settle();
    f.expect(
        feedback::take_recorded() == vec![soft.clone()],
        "one new squat is not exactly one soft tick",
    );
    f.settle();
    f.expect(
        feedback::take_recorded().is_empty(),
        "a tick without a new squat",
    );
    super::challenge_run::inject_counts([0, 0, 4]);
    f.settle();
    f.expect(
        feedback::take_recorded() == vec![soft],
        "several squats in one poll are not one soft tick",
    );
    super::challenge_run::inject_counts([0, 0, 10]);
    f.settle();
    f.expect(
        feedback::take_recorded() == vec![beaten],
        "the last squat does not play only the beaten pattern",
    );
    super::challenge_run::inject_counts([0; 3]);
    f.tap("Exit");
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
    // The field holds the cut text, not what was sent.
    let held = f.type_into(0, &"x".repeat(130));
    f.expect(
        held == format!("TEXT:{}", "x".repeat(120)),
        format!("the field holds [{held}], not the cut text"),
    );
    f.settle();
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

/// An alarm twenty minutes after the clock of these tests, locked from half an hour before, that
/// asks for two squats.
fn locked_alarm(id: &str, label: &str) -> AlarmData {
    let mut a = daily(id, 10, 20, label);
    a.lock_minutes = 30;
    a.challenges = vec![("challenge.squat".to_owned(), r#"{"count":2}"#.to_owned())];
    a
}

impl Flow {
    fn said_locked(&self) -> bool {
        self.has_part("Locked until it rings at") && self.has("Do its challenges")
    }
    fn close_sheets(&self) {
        for _ in 0..2 {
            if self.sheet_open() || self.editor_sheet_open() {
                self.back();
            }
        }
    }
    /// Do the two squats of the unlock run and wait out the celebration.
    fn do_the_squats(&self) {
        let run = self.ui.global::<Run>();
        self.expect(run.get_active(), "the unlock did not start");
        super::challenge_run::inject_counts([0, 0, 2]);
        self.settle();
        i_slint_backend_testing::mock_elapsed_time(std::time::Duration::from_millis(2500));
        self.expect(!run.get_active(), "the unlock did not end after its squats");
        super::challenge_run::inject_counts([0; 3]);
    }
}

/// The edit lock: the switch says no with a sheet, the challenges open the alarm, it can be
/// switched off, and after five minutes it locks again.
#[test]
fn edit_lock_switch_off_unlock_and_lock_again() {
    crate::edit_lock::relock("lk2");
    let f = Flow::new(vec![locked_alarm("lk2", "Lock")]);
    f.expect(f.has("Edit lock is on"), "no lock mark on the card");
    f.expect(f.tap("On or off: Lock"), "no switch");
    f.expect(saved("lk2").enabled, "the switch turned a locked alarm off");
    f.expect(f.said_locked(), format!("no sheet: {:?}", f.texts()));
    f.expect(f.tap("Keep it locked"), "no way to keep it locked");
    f.expect(!f.sheet_open(), "the sheet stayed");
    // The challenges.
    f.expect(f.tap("On or off: Lock"), "no switch the second time");
    super::challenge_run::inject_counts([0, 0, 0]);
    f.expect(f.tap("Do its challenges"), "no way to the challenges");
    f.do_the_squats();
    f.expect(
        f.sn("toast_visible") == true && f.has_part("Open for 5 minutes"),
        format!("nothing said about the unlock: {:?}", f.texts()),
    );
    f.expect(
        !f.has("Edit lock is on"),
        "the mark stayed on an open alarm",
    );
    f.expect(f.tap("On or off: Lock"), "no switch once open");
    f.expect(
        !saved("lk2").enabled,
        "an open alarm cannot be switched off",
    );
    f.expect(f.tap("On or off: Lock"), "no switch to turn it on");
    f.expect(saved("lk2").enabled, "not on again");
    // Five minutes on it locks again.
    crate::clock::set_fixed(Some(wednesday_ten() + 5 * 60_000 + 1));
    home::refresh(&f.ctx);
    f.expect(f.has("Edit lock is on"), "no mark after the five minutes");
    f.expect(f.tap("On or off: Lock"), "no switch after the five minutes");
    f.expect(saved("lk2").enabled, "locked again but switched off");
    f.expect(f.said_locked(), "no sheet after the five minutes");
    crate::edit_lock::relock("lk2");
    f.finish();
}

/// What is blocked from the card's menu and the pause button, and what is not.
#[test]
fn edit_lock_blocks_delete_skip_and_pause_but_not_duplicate() {
    crate::edit_lock::relock("lk3");
    let mut other = daily("b", 7, 0, "Other");
    other.order = 2;
    let mut locked = locked_alarm("lk3", "Lock");
    locked.order = 1;
    let f = Flow::new(vec![locked, other]);
    // The first card is the one that rings first: the locked one.
    for (row, what) in [
        ("Skip the next ring", "skip"),
        ("Pause until a date", "pause"),
        ("Delete", "delete"),
    ] {
        f.close_sheets();
        if what == "skip" {
            // There is no AlarmKit here to say when it rings, so no Skip row: its swipe.
            home::skip_from_swipe(&f.ctx, "lk3");
            f.settle();
        } else {
            f.expect(f.tap_has("More actions"), "no menu");
            f.expect(f.has(row), format!("no {what} row: {:?}", f.texts()));
            f.tap(row);
        }
        f.expect(
            f.said_locked(),
            format!("{what} did not say it is locked: {:?}", f.texts()),
        );
        let a = saved("lk3");
        f.expect(
            a.skip_until_ms.is_none() && a.pause_until_ms.is_none() && a.enabled,
            format!("{what} changed a locked alarm: {a:?}"),
        );
        f.expect(
            crate::persistence::load_alarms()
                .iter()
                .any(|a| a.id == "lk3"),
            format!("{what} deleted a locked alarm"),
        );
    }
    // Pause all names the alarm that is in the way.
    f.close_sheets();
    f.expect(f.tap("Pause all alarms until a date"), "no pause button");
    f.expect(f.tap("For a week"), "no week");
    f.expect(
        f.has_part("Cannot pause all: Lock is locked"),
        format!("pause all said: {:?}", f.texts()),
    );
    f.expect(
        saved("b").pause_until_ms.is_none() && saved("lk3").pause_until_ms.is_none(),
        "pause all paused something while an alarm was locked",
    );
    // Duplicating is not blocked.
    f.close_sheets();
    f.expect(f.tap_has("More actions"), "no menu");
    f.expect(f.tap("Duplicate"), "no Duplicate");
    f.expect(f.screen() == 1, "duplicating a locked alarm was blocked");
    f.finish();
}

/// Saving an edit of a locked alarm, and deleting it from the editor, say no with the sheet of the
/// editor; after its challenges the same save goes through.
#[test]
fn edit_lock_blocks_saving_an_edit_until_the_challenges_are_done() {
    crate::edit_lock::relock("lk5");
    let f = Flow::new(vec![locked_alarm("lk5", "Lock")]);
    f.press("open", "lk5");
    f.expect(f.screen() == 1, "no editor");
    f.expect(f.tap("+1"), "no +1");
    f.expect(f.tap("Save"), "no Save");
    f.expect(f.screen() == 1, "a locked alarm was saved");
    f.expect(
        f.editor_sheet_open() && f.said_locked(),
        format!("no sheet in the editor: {:?}", f.texts()),
    );
    f.expect(saved("lk5").time_minute == 20, "the edit was saved");
    f.expect(f.tap("Keep it locked"), "no way to keep it locked");
    // Delete from the editor.
    f.expect(f.tap("Delete alarm"), "no delete button");
    f.expect(f.tap_last("Delete"), "no confirm");
    f.expect(
        crate::persistence::load_alarms()
            .iter()
            .any(|a| a.id == "lk5"),
        "deleted from the editor while locked",
    );
    f.expect(f.said_locked(), "no sheet after delete");
    // Its challenges, then the same save goes through.
    super::challenge_run::inject_counts([0, 0, 0]);
    f.expect(f.tap("Do its challenges"), "no way to the challenges");
    f.do_the_squats();
    f.expect(f.screen() == 1, "the unlock left the editor");
    f.expect(f.tap("Save"), "no Save after the unlock");
    f.expect(f.screen() == 0, "saving after the unlock did not leave");
    f.expect(saved("lk5").time_minute == 21, "the edit was not saved");
    crate::edit_lock::relock("lk5");
    f.finish();
}

/// An alarm without challenges unlocks through the holds, and an alarm outside its window is not
/// locked.
#[test]
fn edit_lock_without_challenges_needs_the_holds_and_outside_the_window_nothing() {
    crate::edit_lock::relock("lk7");
    let mut bare = locked_alarm("lk7", "Lock");
    bare.challenges.clear();
    let mut far = locked_alarm("far7", "Far");
    far.time_hour = 11;
    far.time_minute = 0;
    let f = Flow::new(vec![bare, far]);
    f.expect(f.tap("On or off: Far"), "no switch of the far alarm");
    f.expect(
        !saved("far7").enabled,
        "an alarm 60 minutes away was locked",
    );
    f.expect(f.tap("On or off: Lock"), "no switch");
    f.expect(f.tap("Do its challenges"), "no way to the challenges");
    let run = f.ui.global::<Run>();
    f.expect(run.get_active(), "the holds did not start");
    f.expect(run.get_total() == 1, format!("total {}", run.get_total()));
    f.expect(f.has_part("20"), format!("not 20 holds: {:?}", f.texts()));
    // Leaving the run changes nothing.
    f.back();
    f.expect(!run.get_active(), "Back left the run up");
    f.expect(saved("lk7").enabled, "leaving the unlock switched it off");
    crate::edit_lock::relock("lk7");
    f.finish();
}

/// The setting: off to begin with, every choice sets its minutes, and Save keeps it.
#[test]
fn edit_lock_setting_sheet_picks_and_saves() {
    let f = Flow::new(Vec::new());
    f.expect(f.tap("Create new alarm"), "no create button");
    f.expect(
        f.has_part("Edit lock") && f.has_part("Off"),
        "the editor has no Edit lock row saying Off",
    );
    f.open_row("Edit lock");
    let steps: [(&str, u16); 10] = [
        ("1 min", 1),
        ("5 min", 5),
        ("10 min", 10),
        ("15 min", 15),
        ("30 min", 30),
        ("1 h", 60),
        ("2 h", 120),
        ("3 h", 180),
        ("6 h", 360),
        ("10 h", 600),
    ];
    for (label, minutes) in steps {
        f.expect(f.tap_last(label), format!("no choice {label}"));
        let got = f
            .ctx
            .editor
            .borrow()
            .as_ref()
            .map(|ed| ed.draft.lock_minutes);
        f.expect(got == Some(minutes), format!("{label} gave {got:?}"));
    }
    f.expect(f.tap_last("Off"), "no Off");
    f.expect(f.tap_last("2 h"), "no 2 h");
    f.done();
    f.expect(
        f.has("2 h before"),
        format!("the summary line: {:?}", f.texts()),
    );
    f.expect(f.tap("Save"), "no Save");
    let kept = crate::persistence::load_alarms()[0].lock_minutes;
    f.expect(kept == 120, format!("saved {kept}"));
    f.finish();
}

/// The language sheet lists every language plus "same as the phone", and each one switches the
/// texts of the app and is saved.
#[test]
fn every_language_can_be_picked() {
    use crate::models::Locale;
    let f = Flow::new(vec![daily("a", 14, 5, "a")]);
    f.expect(f.tap_last("Settings"), "no Settings tab");
    for locale in Locale::ALL {
        f.expect(f.tap_has("Language"), "no Language row");
        f.expect(f.panel_open(), "Language opens no panel");
        let texts = f.texts();
        for other in Locale::ALL {
            f.expect(
                texts.iter().any(|t| t == other.display_name()),
                format!("no {} in the picker: {texts:?}", other.display_name()),
            );
        }
        f.expect(
            texts.iter().any(|t| t == "Same as the phone"),
            format!("no follow-the-phone row: {texts:?}"),
        );
        f.expect(
            f.tap(locale.display_name()),
            format!("cannot pick {}", locale.display_name()),
        );
        f.expect(!f.panel_open(), "the sheet stays open after a choice");
        let tr = crate::i18n::load_translations(locale);
        let settings = tr["nav.settings"].clone();
        f.expect(
            f.has(&settings),
            format!("{locale:?} did not take: {:?}", f.texts()),
        );
        let saved = crate::persistence::load_settings();
        f.expect(saved.locale == locale, format!("saved {:?}", saved.locale));
        f.expect(!saved.prefs.language_follows_phone, "still follows phone");
        // The language row of the settings screen is now in that language; English again.
        f.expect(
            f.tap_has(&tr["settings.language"]),
            format!("{locale:?}: no language row: {:?}", f.texts()),
        );
        f.expect(f.tap("English"), "English does not come back");
        f.expect(f.has("Settings"), "English did not take");
    }
    f.finish();
}

/// A tap through the app is no more than a finger can do: not outside the window, not under the keyboard (the window keeps its
/// full height with the keyboard up), and what is on screen above it is still touched.
#[test]
fn a_tap_through_the_app_stops_where_a_finger_cannot_reach() {
    let f = Flow::new(vec![daily("a", 7, 0, "Work")]);
    let said = |c: &str| f.cmd(c);
    f.expect(said("tapxy:100 100") == "OK", "no touch inside the window");
    for outside in [
        "tapxy:100 6000",
        "tapxy:1080 100",
        "tapxy:-1 100",
        "tapxy:100 -1",
    ] {
        f.expect(
            said(outside).starts_with("ERR outside the window"),
            format!("{outside}: {}", said(outside)),
        );
    }
    // The keyboard: 400 logical pixels at scale 3 = 1200 px of the 6000.
    f.ui.global::<Insets>().set_bottom(400.0);
    f.expect(
        said("tapxy:100 4799") == "OK",
        "no touch just above the keyboard",
    );
    for under in ["tapxy:100 4800", "tapxy:100 5999"] {
        f.expect(
            said(under).starts_with("ERR under the keyboard"),
            format!("{under}: {}", said(under)),
        );
    }
    // The bars alone are no keyboard: a touch there is the app's.
    f.ui.global::<Insets>().set_bottom(40.0);
    f.expect(
        said("tapxy:100 5990") == "OK",
        "a touch in the bar's inset is refused",
    );
    f.ui.global::<Insets>().set_bottom(0.0);
    f.finish();
}

/// Run a held tap (`tapwait:<ms>:<command>`) the way the channel does: the answer comes from the app's own timer. Time is
/// moved by the testing backend in steps of 30 ms and the clock of the wait is real, so each step also lets 30 ms pass.
fn held_tap(f: &Flow, spec: &str, meanwhile: impl Fn(u32)) -> (String, std::time::Duration) {
    let (send, answers) = std::sync::mpsc::channel();
    let started = std::time::Instant::now();
    debug_elements::start_tap_wait(f.ui.as_weak(), spec, move |said| {
        let _ = send.send(said);
    });
    for step in 0..400 {
        if let Ok(said) = answers.try_recv() {
            return (said, started.elapsed());
        }
        meanwhile(step);
        i_slint_backend_testing::mock_elapsed_time(std::time::Duration::from_millis(30));
        std::thread::sleep(std::time::Duration::from_millis(30));
    }
    panic!("no answer to {spec}");
}

/// A tap that waits for its element: the element comes up a moment after the tap was asked for, and the tap lands on it.
#[test]
fn a_held_tap_lands_when_its_element_appears_after_a_moment() {
    let f = Flow::new(vec![daily("a", 7, 0, "Work")]);
    // "Theme" is a row of the settings screen, which is not open yet.
    f.expect(!f.has_part("Theme"), "Theme is up before Settings is open");
    let opened = std::cell::Cell::new(false);
    let (said, took) = held_tap(&f, "2000:tapc:Theme", |step| {
        if step == 3 && !opened.replace(true) {
            f.expect(
                debug_elements::run(&f.ui, "tap@L:Settings").as_deref() == Some("OK"),
                "no Settings tab",
            );
        }
    });
    f.expect(said == "OK", format!("the held tap said {said}"));
    f.expect(
        took < std::time::Duration::from_millis(1500),
        format!("the held tap took {took:?}"),
    );
    f.expect(
        f.ui.get_screen() == 5,
        format!(
            "the tap on Theme did not open the theme screen: screen {}",
            f.ui.get_screen()
        ),
    );
    f.finish();
}

/// An element that is there at once is tapped at once, with no waiting in the answer.
#[test]
fn a_held_tap_on_an_element_that_is_there_answers_at_once() {
    let f = Flow::new(vec![daily("a", 7, 0, "Work")]);
    let (said, took) = held_tap(&f, "2000:tap@L:Settings", |_| {});
    f.expect(said == "OK", format!("the held tap said {said}"));
    f.expect(
        took < std::time::Duration::from_millis(200),
        format!("the held tap took {took:?}"),
    );
    f.finish();
}

/// An element that never comes: the answer is `none`, after the time given and not before.
#[test]
fn a_held_tap_on_an_element_that_never_comes_says_none_after_its_time() {
    let f = Flow::new(vec![daily("a", 7, 0, "Work")]);
    let (said, took) = held_tap(&f, "300:tap:No such text on this screen", |_| {});
    f.expect(said == "none", format!("the held tap said {said}"));
    f.expect(
        took >= std::time::Duration::from_millis(300)
            && took < std::time::Duration::from_millis(1500),
        format!("the held tap took {took:?} for 300 ms"),
    );
    f.finish();
}

/// What a finger cannot reach is refused at once, not waited for; a tap that is no tap is refused at once too.
#[test]
fn a_held_tap_does_not_wait_where_a_finger_cannot_reach() {
    let f = Flow::new(vec![daily("a", 7, 0, "Work")]);
    f.ui.global::<Insets>().set_bottom(400.0);
    // The row of the alarm is in the top part of the 6000 px window; the keyboard covers y >= 4800.
    let (said, took) = held_tap(&f, "2000:tapxy:100 5000", |_| {});
    f.expect(
        said.starts_with("ERR under the keyboard") && took < std::time::Duration::from_millis(200),
        format!("tapxy under the keyboard: {said} after {took:?}"),
    );
    f.ui.global::<Insets>().set_bottom(0.0);
    let (said, _) = held_tap(&f, "2000:find:Settings", |_| {});
    f.expect(said.starts_with("ERR"), format!("find is no tap: {said}"));
    f.finish();
}

/// The size of the emulator suite's phone (1080 x 1920 at 2.625); returns the scale.
fn phone_window(f: &Flow) -> f32 {
    use slint::platform::WindowEvent;
    let scale = 2.625f32;
    let window = f.ui.window();
    window.dispatch_event(WindowEvent::ScaleFactorChanged {
        scale_factor: scale,
    });
    window.set_size(slint::PhysicalSize::new(1080, 1920));
    window.dispatch_event(WindowEvent::Resized {
        size: slint::LogicalSize::new(1080.0 / scale, 1920.0 / scale),
    });
    f.settle();
    scale
}

/// The sheet of the accent colour on a phone-sized window, with the keyboard animated up and down
/// the way Android does it (the area it covers changes in many small steps).
struct PhoneSheet {
    f: Flow,
    scale: f32,
}

impl PhoneSheet {
    fn open() -> PhoneSheet {
        let f = Flow::new(vec![daily("a", 7, 0, "a")]);
        let scale = phone_window(&f);
        assert!(f.tap_last("Settings"));
        assert!(f.tap_has("Theme"));
        assert!(f.tap("Sand"));
        assert!(f.tap_has("Choose your own colour"));
        PhoneSheet { f, scale }
    }

    fn keyboard_in(&self, from: f32, to: f32, steps: usize) {
        for step in 1..=steps {
            let height = from + (to - from) * step as f32 / steps as f32;
            self.f.ui.window().set_virtual_keyboard(
                slint::LogicalPosition::new(0.0, (1920.0 - height) / self.scale),
                slint::LogicalSize::new(1080.0 / self.scale, height / self.scale),
                i_slint_core::InternalToken,
            );
            i_slint_backend_testing::mock_elapsed_time(std::time::Duration::from_millis(16));
        }
    }

    fn done_y(&self) -> Option<i64> {
        let rows: Vec<serde_json::Value> = serde_json::from_str(&self.f.cmd("texts")).unwrap();
        rows.iter()
            .find(|r| r["t"] == "Done")
            .and_then(|r| r["y"].as_i64())
    }
}

/// R-8 (s61): with the keyboard up the sheet's content is taller and scrolls; when the keyboard went
/// down while that scroll was still settling, the content stayed a few points off its limit. The
/// next touch put it back between a press and its release, so the release fell beside the button
/// and Done did nothing. A finger meets this as a Done that needs a second tap.
#[test]
fn done_on_the_colour_sheet_works_after_the_keyboard_went_down() {
    for steps in [3usize, 5] {
        let p = PhoneSheet::open();
        let rest = p.done_y();
        p.f.cmd("settext:0:#c06020");
        let kb = |from: f32, to: f32, wait: u64| {
            p.keyboard_in(from, to, steps);
            i_slint_backend_testing::mock_elapsed_time(std::time::Duration::from_millis(wait));
        };
        kb(0.0, 750.0, 600);
        kb(750.0, 0.0, 0);
        kb(0.0, 750.0, 0);
        kb(750.0, 0.0, 1500);
        p.f.expect(
            p.done_y() == rest,
            format!("{steps} steps: Done at {:?}, not at {rest:?}", p.done_y()),
        );
        p.f.expect(p.f.tap("Done"), "no Done");
        p.f.expect(
            !p.f.ui.global::<Thm>().get_sheet_open(),
            format!("{steps} steps: the sheet is still up after Done"),
        );
        p.f.finish();
    }
}

/// R-8b: the alarm list is a scroll area too (not a Sheet). When the window gets taller (rotation, split
/// screen) while a flick of the list towards its end is still running, the flick ended at the old end,
/// past the new one, and the first touch on a card moved the list between the press and the release, so
/// the switch was not hit.
#[test]
fn the_first_tap_on_a_card_lands_after_the_window_grew_during_a_flick_to_the_end() {
    let scale = 2.625f32;
    for wait in [0u64, 4, 8, 12, 16, 24, 40] {
        let alarms: Vec<AlarmData> = (0..12)
            .map(|i| daily(&format!("a{i}"), 5, i as u8, &format!("alarm {i}")))
            .collect();
        let f = Flow::new(alarms);
        phone_window(&f);
        let window = f.ui.window();
        let at = |y: f32| slint::LogicalPosition::new(100.0, y);
        let left = slint::platform::PointerEventButton::Left;
        window.dispatch_event(slint::platform::WindowEvent::PointerPressed {
            position: at(600.0),
            button: left,
        });
        for step in 1..=8 {
            i_slint_backend_testing::mock_elapsed_time(std::time::Duration::from_millis(8));
            window.dispatch_event(slint::platform::WindowEvent::PointerMoved {
                position: at(600.0 - 40.0 * step as f32),
            });
        }
        window.dispatch_event(slint::platform::WindowEvent::PointerReleased {
            position: at(280.0),
            button: left,
        });
        i_slint_backend_testing::mock_elapsed_time(std::time::Duration::from_millis(wait));
        window.set_size(slint::PhysicalSize::new(1080, 2400));
        window.dispatch_event(slint::platform::WindowEvent::Resized {
            size: slint::LogicalSize::new(1080.0 / scale, 2400.0 / scale),
        });
        f.settle();
        f.expect(saved("a11").enabled, "a11 starts on");
        f.expect(f.tap("On or off: alarm 11"), "no switch on the last card");
        f.expect(
            !saved("a11").enabled,
            format!("{wait} ms: the first tap on the last card did not reach its switch"),
        );
        f.finish();
    }
}

/// Every text input on the screen, found by walking the elements, that has no accessible label.
/// Android's adapter shows no placeholder as a hint, so the label is the only name an empty field has.
fn unnamed_inputs(f: &Flow, screen: &str, found: &mut BTreeSet<String>, seen: &mut usize) {
    use i_slint_backend_testing::{AccessibleRole, ElementQuery};
    let inputs = ElementQuery::from_root(&f.ui)
        .match_descendants()
        .match_accessible_role(AccessibleRole::TextInput)
        .find_all();
    for input in inputs {
        *seen += 1;
        if input.accessible_label().is_none_or(|l| l.trim().is_empty()) {
            let id = input.id().map(|i| i.to_string()).unwrap_or_default();
            found.insert(format!("{screen}: {id} has no label"));
        }
    }
}

/// A person who cannot see the screen lands on a text field and hears its name: no field of any
/// screen is nameless while it is empty. New fields are found by walking, not listed by hand.
#[test]
fn every_text_field_has_a_name_for_a_screen_reader() {
    let f = Flow::new(Vec::new());
    let mut found = BTreeSet::new();
    let mut seen = 0;
    f.expect(f.tap("Create new alarm"), "no create button");
    unnamed_inputs(&f, "editor", &mut found, &mut seen);
    for row in ["Label", "Vibration", "Snooze", "Sound"] {
        f.open_row(row);
        unnamed_inputs(&f, row, &mut found, &mut seen);
        if f.has("Done") {
            f.done();
        }
    }
    f.expect(f.tap_has("Time:"), "no time button");
    f.expect(f.tap("Type it"), "no Type it");
    unnamed_inputs(&f, "time sheet", &mut found, &mut seen);
    f.done();
    f.finish();
    for challenge in ["Scan", "Typing"] {
        let c = Flow::new(Vec::new());
        c.expect(c.tap("Create new alarm"), "no create button");
        c.open_row("Challenges");
        c.expect(c.tap("Add a challenge"), "no add");
        c.expect(c.tap_last(challenge), format!("no {challenge}"));
        unnamed_inputs(&c, challenge, &mut found, &mut seen);
        if challenge == "Typing" {
            c.expect(c.tap("Try"), "no Try");
            unnamed_inputs(&c, "typing run", &mut found, &mut seen);
        }
        c.finish();
    }
    let g = Flow::new(Vec::new());
    g.expect(g.tap_last("History"), "no History tab");
    g.press("h-period", "6");
    unnamed_inputs(&g, "history period", &mut found, &mut seen);
    g.finish();
    assert!(seen >= 5, "the walk found only {seen} text inputs");
    assert!(found.is_empty(), "nameless text fields: {found:#?}");
}

/// A test alarm that rings while the reliability screen stays open (the phone locked, the ring dismissed, the
/// window back on the same screen): the page says so again, with the "Test alarm" button and not "Cancel".
#[test]
fn reliability_page_follows_a_test_alarm_that_rang() {
    let f = Flow::new(Vec::new());
    reliability::open(&f.ctx, false);
    f.settle();
    f.expect(f.screen() == SCREEN_RELIABILITY, "no reliability screen");
    f.expect(f.tap("Test alarm"), "no Test alarm button");
    f.expect(f.has("Cancel the test alarm"), "no Cancel while it waits");
    // The ring ends: the app loads its alarms again (ring::poll) and the test alarm is gone from them.
    f.ctx.alarms.borrow_mut().retain(|alarm| !alarm.test);
    f.settle();
    f.settle();
    f.expect(
        f.has("Test alarm") && !f.has("Cancel the test alarm"),
        format!("after the ring: {:?}", f.texts()),
    );
    f.finish();
}
