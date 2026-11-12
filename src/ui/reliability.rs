//! The reliability screen and the banner on Home: what the phone has to allow for alarms to
//! ring on time, how each thing is fixed, and the test alarm. Also the first-run setup, which
//! is this list with a word of welcome. The checks themselves are in `crate::reliability`.

use std::rc::Rc;
use std::time::Duration;

use mobile_sentinel::health::{self, Check, HealthReport};
use slint::{ComponentHandle, SharedString};

use super::editor::model;
use super::{CheckRow, Ctx, Rel, SCREEN_RELIABILITY, SCREEN_SETTINGS};
use crate::i18n::t;
use crate::reliability::{
    can_hide, check_name, check_text, first_problem, last_ring, report, CHECKS,
};

/// A test alarm rings about this long from now.
const TEST_SECONDS: i64 = 60;

#[derive(Default)]
pub struct State {
    /// The screen to go back to.
    from: i32,
    first_run: bool,
    /// The checks as listed, to know which row was tapped.
    order: Vec<Check>,
    last: Option<HealthReport>,
    timer: slint::Timer,
}

// ---- The banner on Home ----------------------------------------------------------------

/// Look again at what the phone allows, and show the most serious problem on Home.
pub fn refresh_banner(ctx: &Ctx) {
    let hidden = ctx.settings.borrow().prefs.hidden_warnings.clone();
    ctx.problem.set(first_problem(&hidden));
    show_banner(ctx);
}

/// Draw the banner from the problem found last, in the language of now.
pub fn show_banner(ctx: &Ctx) {
    let Some(ui) = ctx.ui.upgrade() else {
        return;
    };
    match ctx.problem.get() {
        Some(check) => {
            let tr = ctx.tr.borrow();
            let (name, _) = check_text(&tr, check);
            ui.set_banner(
                t(&tr, "reliability.banner")
                    .replace("{check}", &name)
                    .into(),
            );
            ui.set_banner_can_hide(can_hide(check));
        }
        None => {
            ui.set_banner(SharedString::new());
            ui.set_banner_can_hide(false);
        }
    }
}

pub fn hide_banner(ctx: &Ctx) {
    if let Some(check) = ctx.problem.get().filter(|check| can_hide(*check)) {
        ctx.settings
            .borrow_mut()
            .prefs
            .hidden_warnings
            .push(check_name(check).to_owned());
        crate::persistence::save_settings(&ctx.settings.borrow());
        refresh_banner(ctx);
    }
}

// ---- The screen ------------------------------------------------------------------------

/// Show the screen. `first_run` is the setup before anything else.
pub fn open(ctx: &Rc<Ctx>, first_run: bool) {
    let Some(ui) = ctx.ui.upgrade() else {
        return;
    };
    {
        let mut state = ctx.reliability.borrow_mut();
        state.from = if first_run { 0 } else { ui.get_screen() };
        state.first_run = first_run;
        state.last = None;
        // The user changes these on the system's pages and comes back.
        let weak = Rc::downgrade(ctx);
        state.timer.start(
            slint::TimerMode::Repeated,
            Duration::from_millis(1500),
            move || {
                if let Some(ctx) = weak.upgrade() {
                    let fresh = report();
                    let changed = ctx.reliability.borrow().last.as_ref() != Some(&fresh);
                    if changed {
                        sync(&ctx);
                    }
                }
            },
        );
    }
    sync(ctx);
    ui.set_panel_open(false);
    ui.set_screen(SCREEN_RELIABILITY);
}

fn sync(ctx: &Ctx) {
    let Some(ui) = ctx.ui.upgrade() else {
        return;
    };
    let tr = ctx.tr.borrow();
    let health = report();
    // What needs doing first.
    let mut checks = CHECKS.to_vec();
    checks.sort_by_key(|check| health.is_ok(*check));
    let kit = crate::platform::alarm_kit_optional();
    let log = kit.map(|kit| kit.timing_log()).unwrap_or_default();
    let last_test = last_ring(&log)
        .map(|(_, took_ms)| {
            t(&tr, "reliability.last_ring")
                .replace("{seconds}", &format!("{:.1}", took_ms as f64 / 1000.0))
        })
        .unwrap_or_default();
    let rel = ui.global::<Rel>();
    {
        let state = ctx.reliability.borrow();
        rel.set_first_run(state.first_run);
    }
    rel.set_checks(model(
        checks
            .iter()
            .map(|check| {
                let ok = health.is_ok(*check);
                let (title, note) = check_text(&tr, *check);
                CheckRow {
                    title: title.into(),
                    note: note.into(),
                    ok,
                    // No app can open the maker's page for this one.
                    fixable: !ok && *check != Check::BootSignal,
                }
            })
            .collect(),
    ));
    rel.set_help(
        format!(
            "{} {} {last_test}",
            t(&tr, "reliability.intro"),
            t(&tr, "reliability.test_note"),
        )
        .trim_end()
        .into(),
    );
    rel.set_has_record(!log.is_empty());
    rel.set_test_pending(ctx.alarms.borrow().iter().any(|alarm| alarm.test));
    let mut state = ctx.reliability.borrow_mut();
    state.order = checks;
    state.last = Some(health);
}

pub fn fix(ctx: &Ctx, index: usize) {
    let Some(check) = ctx.reliability.borrow().order.get(index).copied() else {
        return;
    };
    // Notifications are asked for in place first, which only works while Android has not
    // been told no.
    if check == Check::Notifications {
        let _ = mobile_sentinel::permissions::request("android.permission.POST_NOTIFICATIONS");
        return;
    }
    // One tap in Android's own dialog, instead of a list of every app on the phone.
    if check == Check::BatteryUnrestricted {
        mobile_sentinel::battery::request_exemption();
        return;
    }
    health::open_settings(check, crate::platform::FOREGROUND_CHANNEL_ID);
}

pub fn app_settings() {
    mobile_sentinel::permissions::open_app_settings();
}

pub fn save_record(ctx: &Rc<Ctx>) {
    let record = crate::platform::alarm_kit_optional()
        .map(|kit| kit.timing_log())
        .unwrap_or_default();
    let saved = crate::platform::save_to_downloads(&record, "alarmfree-rings.txt", "text/plain");
    match saved {
        Ok(place) => {
            let message = t(&ctx.tr.borrow(), "settings.backup_saved").replace("{place}", &place);
            super::toast::show(ctx, message, None);
        }
        Err(e) => {
            log::warn!("[AlarmFree] The ring record was not saved: {e}");
            let message = t(&ctx.tr.borrow(), "settings.backup_failed");
            super::toast::error(ctx, message);
        }
    }
}

/// A hidden alarm in a minute: the only check that holds on any phone.
pub fn test(ctx: &Rc<Ctx>) {
    let now = chrono::Local::now().naive_local();
    let label = t(&ctx.tr.borrow(), "reliability.test_label");
    let alarm = crate::alarm_ops::test_alarm(now, TEST_SECONDS, label);
    ctx.alarms.borrow_mut().push(alarm);
    crate::persistence::save_alarms(&ctx.alarms.borrow());
    let message = t(&ctx.tr.borrow(), "reliability.test_set");
    super::toast::show(ctx, message, None);
    sync(ctx);
}

pub fn cancel_test(ctx: &Rc<Ctx>) {
    ctx.alarms.borrow_mut().retain(|alarm| !alarm.test);
    crate::persistence::save_alarms(&ctx.alarms.borrow());
    let message = t(&ctx.tr.borrow(), "reliability.test_cancelled");
    super::toast::show(ctx, message, None);
    sync(ctx);
}

/// Back, or the arrow. The setup also counts as done.
pub fn leave(ctx: &Rc<Ctx>) {
    let Some(ui) = ctx.ui.upgrade() else {
        return;
    };
    let (from, first_run) = {
        let state = ctx.reliability.borrow();
        state.timer.stop();
        (state.from, state.first_run)
    };
    if first_run {
        ctx.settings.borrow_mut().prefs.setup_done = true;
        crate::persistence::save_settings(&ctx.settings.borrow());
    }
    ui.set_screen(from);
    // What the user changed on the system's pages shows on the way back.
    refresh_banner(ctx);
    if from == SCREEN_SETTINGS {
        super::settings::refresh(ctx);
    }
    super::home::refresh(ctx);
}

pub fn connect(ctx: &Rc<Ctx>, ui: &super::MainWindow) {
    let rel = ui.global::<Rel>();
    let weak = Rc::downgrade(ctx);
    rel.on_back({
        let weak = weak.clone();
        move || {
            if let Some(ctx) = weak.upgrade() {
                leave(&ctx);
            }
        }
    });
    rel.on_done({
        let weak = weak.clone();
        move || {
            if let Some(ctx) = weak.upgrade() {
                leave(&ctx);
            }
        }
    });
    rel.on_fix({
        let weak = weak.clone();
        move |i| {
            if let Some(ctx) = weak.upgrade() {
                fix(&ctx, i as usize);
            }
        }
    });
    rel.on_app_settings(app_settings);
    rel.on_save_record({
        let weak = weak.clone();
        move || {
            if let Some(ctx) = weak.upgrade() {
                save_record(&ctx);
            }
        }
    });
    rel.on_test({
        let weak = weak.clone();
        move || {
            if let Some(ctx) = weak.upgrade() {
                test(&ctx);
            }
        }
    });
    rel.on_cancel_test(move || {
        if let Some(ctx) = weak.upgrade() {
            cancel_test(&ctx);
        }
    });
}
