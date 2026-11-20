//! Help, the report form, the intro, the rate row and the banner after trouble. The report is
//! built in `crate::report`; the email app is opened with `share::email`, and nothing leaves the
//! phone before the user presses send in it.

use std::rc::Rc;

use slint::ComponentHandle;

use super::editor::model;
use super::{
    Ctx, Fb, HelpItem, Hp, Intro, SCREEN_HELP, SCREEN_INTRO, SCREEN_REPORT, SCREEN_SETTINGS,
};
use crate::i18n::t;
use crate::report::{self, Answers, Kind, Parts};

/// Where AlarmFree is on Google Play.
const PLAY_ID: &str = "com.fidderr.alarmfree";

const CHALLENGE_KINDS: [&str; 9] = [
    "math", "scan", "shake", "steps", "squat", "memory", "typing", "hold", "reaction",
];

#[derive(Default)]
pub struct State {
    /// The screen to go back to from the intro, the report and the help.
    from: i32,
    first_run: bool,
    kind: Kind,
    attach_log: bool,
    attach_setup: bool,
}

fn open_screen(ctx: &Ctx, screen: i32) {
    if let Some(ui) = ctx.ui.upgrade() {
        let current = ui.get_screen();
        if ![SCREEN_INTRO, SCREEN_REPORT, SCREEN_HELP].contains(&current) {
            ctx.feedback.borrow_mut().from = current;
        }
        ui.set_panel_open(false);
        ui.set_screen(screen);
    }
}

/// Back to where the screen was opened from.
fn leave(ctx: &Ctx) {
    let Some(ui) = ctx.ui.upgrade() else {
        return;
    };
    let from = ctx.feedback.borrow().from;
    ui.set_screen(from);
    if from == SCREEN_SETTINGS {
        super::settings::refresh(ctx);
    }
    super::home::refresh(ctx);
}

// ---- Intro -----------------------------------------------------------------------------

/// Three cards before the setup, or from the settings when the user asks for them again.
pub fn open_intro(ctx: &Rc<Ctx>, first_run: bool) {
    ctx.feedback.borrow_mut().first_run = first_run;
    if let Some(ui) = ctx.ui.upgrade() {
        ui.global::<Intro>().set_page(0);
    }
    open_screen(ctx, SCREEN_INTRO);
}

/// Skip, the last card, or back: the first run goes on to the setup.
pub fn finish_intro(ctx: &Rc<Ctx>) {
    if ctx.feedback.borrow().first_run {
        ctx.feedback.borrow_mut().first_run = false;
        super::reliability::open(ctx, true);
    } else {
        leave(ctx);
    }
}

// ---- Rate ------------------------------------------------------------------------------

/// The addresses to try, in order: the Play Store app, then its web page.
pub fn rate_urls() -> [String; 2] {
    [
        format!("market://details?id={PLAY_ID}"),
        format!("https://play.google.com/store/apps/details?id={PLAY_ID}"),
    ]
}

pub fn rate(ctx: &Rc<Ctx>) {
    let opened = rate_urls()
        .iter()
        .any(|url| mobile_sentinel::share::open_url(url).is_ok());
    if !opened {
        let message = t(&ctx.tr.borrow(), "settings.rate_failed");
        super::toast::error(ctx, message);
    }
}

// ---- Help ------------------------------------------------------------------------------

fn help_items(ctx: &Ctx) -> Vec<HelpItem> {
    let tr = ctx.tr.borrow();
    let item = |question: &str, answer: String| HelpItem {
        question: t(&tr, question).into(),
        answer: answer.into(),
    };
    let kinds = CHALLENGE_KINDS
        .iter()
        .map(|kind| {
            format!(
                "{}: {}",
                t(&tr, &format!("challenge.{kind}")),
                t(&tr, &format!("challenge.desc.{kind}"))
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    vec![
        item(
            "help.q_ring",
            format!(
                "{}\n\n{}",
                t(&tr, "help.a_ring"),
                t(&tr, "reliability.test_note")
            ),
        ),
        item(
            "help.q_challenges",
            format!("{}\n\n{kinds}", t(&tr, "config.challenges_note")),
        ),
        item(
            "help.q_backup_challenge",
            format!(
                "{}\n\n{}",
                t(&tr, "challenge.fallback.explain"),
                t(&tr, "help.a_backup_challenge")
                    .replace("{holds}", &crate::platform::BACKUP_HOLDS.to_string())
            ),
        ),
        item(
            "help.q_backup",
            format!(
                "{}\n\n{}",
                t(&tr, "settings.backup_note"),
                t(&tr, "help.a_backup")
            ),
        ),
        item("help.q_data", t(&tr, "help.a_data")),
    ]
}

/// Back from the help.
pub fn leave_help(ctx: &Rc<Ctx>) {
    leave(ctx);
}

pub fn open_help(ctx: &Rc<Ctx>) {
    if let Some(ui) = ctx.ui.upgrade() {
        ui.global::<Hp>().set_items(model(help_items(ctx)));
    }
    open_screen(ctx, SCREEN_HELP);
}

// ---- The report ------------------------------------------------------------------------

fn report_dir() -> std::path::PathBuf {
    mobile_sentinel::app_files_dir().join("report")
}

/// Show the form. The "roughly when" starts as the last alarm that rang.
pub fn open_report(ctx: &Rc<Ctx>, kind: Kind) {
    let Some(ui) = ctx.ui.upgrade() else {
        return;
    };
    let _ = std::fs::remove_dir_all(report_dir());
    {
        let mut state = ctx.feedback.borrow_mut();
        state.kind = kind;
        state.attach_log = true;
        state.attach_setup = false;
    }
    let when = report::last_ring_time(&mobile_sentinel::diag::read())
        .map(|time| t(&ctx.tr.borrow(), "report.when_last").replace("{time}", &time))
        .unwrap_or_default();
    let fb = ui.global::<Fb>();
    fb.set_kind(i32::from(kind == Kind::Idea));
    fb.set_what("".into());
    fb.set_expected("".into());
    fb.set_when(when.into());
    fb.set_attach_log(true);
    fb.set_attach_setup(false);
    fb.set_previewing(false);
    fb.set_preview("".into());
    fb.set_note("".into());
    open_screen(ctx, SCREEN_REPORT);
}

fn answers(ctx: &Ctx) -> Answers {
    let kind = ctx.feedback.borrow().kind;
    let Some(ui) = ctx.ui.upgrade() else {
        return Answers::default();
    };
    let fb = ui.global::<Fb>();
    Answers {
        kind,
        what: fb.get_what().into(),
        expected: if kind == Kind::Problem {
            fb.get_expected().into()
        } else {
            String::new()
        },
        when: fb.get_when().into(),
    }
}

/// The report as it would be attached now.
fn full_text(ctx: &Ctx) -> String {
    let (include_log, backup_attached) = {
        let state = ctx.feedback.borrow();
        (state.attach_log, state.attach_setup)
    };
    let alarms = ctx.alarms.borrow();
    let health = crate::reliability::report();
    report::build(
        &answers(ctx),
        env!("CARGO_PKG_VERSION"),
        &Parts {
            device: &mobile_sentinel::diag::device_summary(),
            permissions: &health.summary(),
            alarms: &alarms,
            log: &mobile_sentinel::diag::read(),
            include_log,
            backup_attached,
            created: &crate::clock::local_now().to_rfc3339(),
        },
    )
}

pub fn kind_picked(ctx: &Rc<Ctx>, index: i32) {
    let kind = if index == 1 {
        Kind::Idea
    } else {
        Kind::Problem
    };
    ctx.feedback.borrow_mut().kind = kind;
    if let Some(ui) = ctx.ui.upgrade() {
        ui.global::<Fb>().set_kind(index);
    }
}

pub fn toggle_log(ctx: &Rc<Ctx>) {
    let on = {
        let mut state = ctx.feedback.borrow_mut();
        state.attach_log = !state.attach_log;
        state.attach_log
    };
    if let Some(ui) = ctx.ui.upgrade() {
        ui.global::<Fb>().set_attach_log(on);
    }
}

pub fn toggle_setup(ctx: &Rc<Ctx>) {
    let on = {
        let mut state = ctx.feedback.borrow_mut();
        state.attach_setup = !state.attach_setup;
        state.attach_setup
    };
    if let Some(ui) = ctx.ui.upgrade() {
        ui.global::<Fb>().set_attach_setup(on);
    }
}

pub fn toggle_preview(ctx: &Rc<Ctx>) {
    let Some(ui) = ctx.ui.upgrade() else {
        return;
    };
    let fb = ui.global::<Fb>();
    if fb.get_previewing() {
        fb.set_previewing(false);
    } else {
        let mut text = full_text(ctx);
        if ctx.feedback.borrow().attach_setup {
            text.push_str(&format!(
                "\n== {} ==\n{}",
                report::SETUP_FILE,
                crate::alarm_ops::export_json(&ctx.alarms.borrow())
            ));
        }
        fb.set_preview(text.into());
        fb.set_previewing(true);
    }
}

/// Leave the form, or the preview first.
pub fn back_from_report(ctx: &Rc<Ctx>) {
    let Some(ui) = ctx.ui.upgrade() else {
        return;
    };
    let fb = ui.global::<Fb>();
    if fb.get_previewing() {
        fb.set_previewing(false);
    } else {
        leave(ctx);
    }
}

pub fn send(ctx: &Rc<Ctx>) {
    let Some(ui) = ctx.ui.upgrade() else {
        return;
    };
    let text = full_text(ctx);
    let answers = answers(ctx);
    let attach_setup = ctx.feedback.borrow().attach_setup;
    let dir = report_dir();
    let report_path = dir.join(report::REPORT_FILE);
    let setup_path = dir.join(report::SETUP_FILE);
    let written = std::fs::create_dir_all(&dir)
        .and_then(|()| std::fs::write(&report_path, &text))
        .and_then(|()| {
            if attach_setup {
                std::fs::write(
                    &setup_path,
                    crate::alarm_ops::export_json(&ctx.alarms.borrow()),
                )
            } else {
                Ok(())
            }
        });
    if written.is_err() {
        let message = t(&ctx.tr.borrow(), "report.failed");
        super::toast::error(ctx, message);
        return;
    }
    let subject = report::subject(
        answers.kind,
        env!("CARGO_PKG_VERSION"),
        report::VERSION_CODE,
        &answers.what,
    );
    let body = report::summary(
        &answers,
        env!("CARGO_PKG_VERSION"),
        &crate::clock::local_now().to_rfc3339(),
    );
    let report_path = report_path.to_string_lossy().into_owned();
    let setup_path = setup_path.to_string_lossy().into_owned();
    let mut paths = vec![report_path.as_str()];
    if attach_setup {
        paths.push(setup_path.as_str());
    }
    crate::diag_log::event("report", &format!("email opened: {subject}"));
    match mobile_sentinel::share::email(report::CONTACT_EMAIL, &subject, &body, &paths) {
        Ok(()) => {
            mark_seen(ctx);
            let message = t(&ctx.tr.borrow(), "report.sent");
            leave(ctx);
            super::toast::show(ctx, message, None);
        }
        Err(e) => {
            log::warn!("[AlarmFree] no email app: {e:?}");
            let stamp = crate::clock::local_now().format("%Y%m%d-%H%M");
            let name = format!("alarmfree-report-{stamp}.txt");
            let saved = crate::platform::save_to_downloads(&text, &name, "text/plain");
            if attach_setup {
                let _ = crate::platform::save_to_downloads(
                    &crate::alarm_ops::export_json(&ctx.alarms.borrow()),
                    &format!("alarmfree-setup-{stamp}.json"),
                    "application/json",
                );
            }
            let note = match saved {
                Ok(place) if report::CONTACT_EMAIL.is_empty() => {
                    t(&ctx.tr.borrow(), "report.saved_no_address").replace("{place}", &place)
                }
                Ok(place) => t(&ctx.tr.borrow(), "report.saved")
                    .replace("{place}", &place)
                    .replace("{address}", report::CONTACT_EMAIL),
                Err(_) => t(&ctx.tr.borrow(), "report.failed"),
            };
            ui.global::<Fb>().set_note(note.into());
        }
    }
}

// ---- After trouble ---------------------------------------------------------------------

fn mark_seen(ctx: &Ctx) {
    let stamp = report::newest_stamp(&mobile_sentinel::diag::read());
    ctx.settings.borrow_mut().prefs.report_seen = stamp;
    crate::persistence::save_settings(&ctx.settings.borrow());
    refresh_trouble(ctx);
}

/// Show the banner on Home when the log has a panic or a missed alarm the user has not been
/// asked about.
pub fn refresh_trouble(ctx: &Ctx) {
    let Some(ui) = ctx.ui.upgrade() else {
        return;
    };
    let seen = ctx.settings.borrow().prefs.report_seen.clone();
    let trouble = report::trouble_since(&mobile_sentinel::diag::read(), &seen);
    let text = if trouble {
        t(&ctx.tr.borrow(), "report.trouble")
    } else {
        String::new()
    };
    ui.global::<Fb>().set_trouble(text.into());
}

pub fn connect(ctx: &Rc<Ctx>, ui: &super::MainWindow) {
    macro_rules! on {
        ($global:ty, $setter:ident, |$ctx:ident $(, $arg:ident)*| $body:expr) => {{
            let weak = Rc::downgrade(ctx);
            ui.global::<$global>().$setter(move |$($arg),*| {
                if let Some($ctx) = weak.upgrade() {
                    $body
                }
            });
        }};
    }
    on!(Fb, on_back, |ctx| back_from_report(&ctx));
    on!(Fb, on_kind_picked, |ctx, i| kind_picked(&ctx, i));
    on!(Fb, on_toggle_log, |ctx| toggle_log(&ctx));
    on!(Fb, on_toggle_setup, |ctx| toggle_setup(&ctx));
    on!(Fb, on_toggle_preview, |ctx| toggle_preview(&ctx));
    on!(Fb, on_send, |ctx| send(&ctx));
    on!(Fb, on_open_trouble, |ctx| open_report(&ctx, Kind::Problem));
    on!(Fb, on_hide_trouble, |ctx| mark_seen(&ctx));
    on!(Hp, on_back, |ctx| leave(&ctx));
    on!(Hp, on_open_reliability, |ctx| super::reliability::open(
        &ctx, false
    ));
    on!(Hp, on_open_report, |ctx| open_report(&ctx, Kind::Problem));
    on!(Intro, on_skip, |ctx| finish_intro(&ctx));
    on!(Intro, on_finish, |ctx| finish_intro(&ctx));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rating_tries_the_store_app_then_the_web_page() {
        let [store, web] = rate_urls();
        assert_eq!(store, "market://details?id=com.fidderr.alarmfree");
        assert_eq!(
            web,
            "https://play.google.com/store/apps/details?id=com.fidderr.alarmfree"
        );
    }

    #[test]
    fn the_play_id_is_the_application_id() {
        let toml = include_str!("../../sentinel.toml");
        assert!(toml.contains(&format!("application_id = \"{PLAY_ID}\"")));
    }

    #[test]
    fn every_help_text_the_page_reads_exists_in_every_language() {
        for locale in crate::models::Locale::ALL {
            let tr = crate::i18n::load_translations(locale);
            for kind in CHALLENGE_KINDS {
                assert!(tr.contains_key(&format!("challenge.{kind}")), "{kind}");
                assert!(tr.contains_key(&format!("challenge.desc.{kind}")), "{kind}");
            }
        }
    }
}
