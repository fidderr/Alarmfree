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
    fn with_look(
        mut settings: AppSettings,
        alarms: Vec<AlarmData>,
        scale: f32,
        font: f32,
    ) -> Flow {
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
    crate::history_persistence::query_history(&crate::models::HistoryFilter::LastDays(100_000)).len()
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
    f.expect(
        f.has("Delete"),
        "older than 30 days went without asking",
    );
    f.back();
    f.expect(!f.panel_open(), "Back left the period question open");
    f.expect(history_count() == 2, "Back on the period question deleted");
    f.press("h-delete-history", "");
    f.tap("Older than 30 days");
    f.expect(f.tap("Delete"), "no Delete");
    f.expect(history_count() == 1, format!("older than 30 days left {}", history_count()));
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
    f.expect(history_count() == 0, format!("left {} after confirming", history_count()));
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
    f.expect(f.tap("For a week"), format!("no 'For a week': {:?}", f.texts()));
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
    f.expect(rows.contains("Lofi"), format!("sound not taken over: {rows}"));
    f.expect(rows.contains("9 min, 2"), format!("snooze not taken over: {rows}"));
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
        let f = Flow::with_look(AppSettings::default(), vec![daily("a", 7, 0, "a")], scale, font);
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
    for theme in ["Dark", "Light", "Pure black", "Midnight", "Forest", "Plum", "Sand", "Mist"] {
        f.expect(names.iter().any(|t| t == theme), format!("no theme {theme}"));
    }
    f.expect(f.tap("Sand"), "no Sand");
    f.expect(f.sn("pal_bg") == "#f3ede2", format!("Sand is {}", f.sn("pal_bg")));
    f.expect(f.sn("pal_light") == true, "Sand is not light");
    f.finish();
}
