//! The settings screen: app-wide preferences, and the panels they open.

use std::rc::Rc;

use slint::ComponentHandle;

use super::panel::{self, Act, Panel};
use super::{Ctx, SettingsView, SCREEN_SETTINGS};
use crate::i18n::{self, t, Translations};
use crate::models::{AlarmData, AppSettings, Locale, TimeFormat, WeekStart};

/// The percentages for the minimum volume.
const VOLUMES: [u8; 5] = [0, 25, 50, 75, 100];

fn format_name(format: TimeFormat, tr: &Translations) -> String {
    t(
        tr,
        match format {
            TimeFormat::System => "settings.follow_phone",
            TimeFormat::H24 => "settings.h24",
            TimeFormat::H12 => "settings.h12",
        },
    )
}

fn week_name(start: WeekStart, tr: &Translations) -> String {
    t(
        tr,
        match start {
            WeekStart::Monday => "day.monday",
            WeekStart::Sunday => "day.sunday",
            WeekStart::Saturday => "day.saturday",
        },
    )
}

fn volume_name(percent: u8, tr: &Translations) -> String {
    if percent == 0 {
        t(tr, "settings.volume_off")
    } else {
        format!("{percent}%")
    }
}

/// What the Reliability banner says under its title.
fn reliability_line(tr: &Translations, problems: usize) -> String {
    match problems {
        0 => t(tr, "settings.reliability_fine"),
        1 => t(tr, "settings.reliability_one"),
        n => t(tr, "settings.reliability_many").replace("{count}", &n.to_string()),
    }
}

fn language_name(settings: &AppSettings, tr: &Translations) -> String {
    if settings.prefs.language_follows_phone {
        t(tr, "settings.follow_phone")
    } else {
        settings.locale.display_name().to_owned()
    }
}

/// Show the screen.
pub fn enter(ctx: &Ctx) {
    if let Some(ui) = ctx.ui.upgrade() {
        ui.set_panel_open(false);
        ui.set_screen(SCREEN_SETTINGS);
    }
    refresh(ctx);
}

/// Draw the rows from the current settings.
pub fn refresh(ctx: &Ctx) {
    let Some(ui) = ctx.ui.upgrade() else {
        return;
    };
    let settings = ctx.settings.borrow();
    let tr = ctx.tr.borrow();
    let problems = crate::reliability::report().problems().len();
    ui.set_st(SettingsView {
        theme: t(&tr, settings.theme.name_key()).into(),
        language: language_name(&settings, &tr).into(),
        time_format: format_name(settings.prefs.time_format, &tr).into(),
        week_start: week_name(settings.prefs.week_start, &tr).into(),
        volume: volume_name(settings.min_volume_percent, &tr).into(),
        version: env!("CARGO_PKG_VERSION").into(),
        touch: settings.prefs.touch_feedback,
        motion: settings.prefs.less_motion,
        confetti: settings.prefs.confetti,
        widget: mobile_sentinel::home_widget::can_add(),
        reliability: reliability_line(&tr, problems).into(),
        problems: problems > 0,
    });
}

/// Change a setting: keep it, tell what shows outside the app, and show it everywhere.
pub fn update(ctx: &Ctx, change: impl FnOnce(&mut AppSettings)) {
    change(&mut ctx.settings.borrow_mut());
    crate::persistence::save_settings(&ctx.settings.borrow());
    // The widget speaks the language and writes the time as set here.
    crate::platform::refresh_outside_app();
    show_everywhere(ctx);
}

fn show_everywhere(ctx: &Ctx) {
    super::home::refresh(ctx);
    super::history::refresh_if_shown(ctx);
    refresh(ctx);
}

pub fn set_language(ctx: &Ctx, locale: Option<Locale>) {
    let chosen = locale.unwrap_or_else(|| {
        Locale::from_language_code(&mobile_sentinel::device_language()).unwrap_or_default()
    });
    {
        let mut settings = ctx.settings.borrow_mut();
        settings.prefs.language_follows_phone = locale.is_none();
        settings.locale = chosen;
        crate::persistence::save_settings(&settings);
    }
    *ctx.tr.borrow_mut() = i18n::load_translations(chosen);
    if let Some(ui) = ctx.ui.upgrade() {
        let tr = ui.global::<super::Tr>();
        tr.set_rev(tr.get_rev() + 1);
    }
    crate::platform::set_ring_language(chosen);
    crate::platform::refresh_outside_app();
    show_everywhere(ctx);
}

pub fn toggled(ctx: &Ctx, index: i32, on: bool) {
    match index {
        0 => {
            update(ctx, |s| s.prefs.touch_feedback = on);
            crate::feedback::set_enabled(on);
        }
        1 => {
            update(ctx, |s| s.prefs.less_motion = on);
            apply_motion(ctx);
        }
        2 => update(ctx, |s| s.prefs.confetti = on),
        _ => {}
    }
}

/// The animations take no time when the user asked for less motion; the texts follow the phone's font size.
pub fn apply_motion(ctx: &Ctx) {
    if let Some(ui) = ctx.ui.upgrade() {
        // The phone's own "remove animations" option counts as well.
        let less =
            ctx.settings.borrow().prefs.less_motion || mobile_sentinel::display::animations_off();
        ui.global::<super::Motion>().set_less(less);
        // The phone's font size, like the original's text zoom (the big clocks stay as they are).
        ui.global::<super::Zoom>()
            .set_scale(mobile_sentinel::display::font_scale().clamp(0.8, 2.0));
    }
}

/// A row was tapped.
pub fn row(ctx: &Rc<Ctx>, index: i32) {
    match index {
        0 => open_language(ctx),
        1 => open_time_format(ctx),
        2 => open_week_start(ctx),
        3 => {
            mobile_sentinel::home_widget::add();
        }
        4 => super::editor::open(ctx, "defaults"),
        5 => open_volume(ctx),
        6 => open_backup(ctx),
        7 => super::history::open_delete(ctx),
        8 => open_about(ctx),
        9 => super::reliability::open(ctx, false),
        10 => super::theme::open(ctx),
        11 => super::feedback::open_help(ctx),
        12 => super::feedback::rate(ctx),
        13 => super::feedback::open_report(ctx, crate::report::Kind::Problem),
        14 => super::feedback::open_intro(ctx, false),
        _ => {}
    }
}

fn open_language(ctx: &Ctx) {
    let tr = ctx.tr.borrow();
    let settings = ctx.settings.borrow();
    let mut rows = vec![(
        t(&tr, "settings.follow_phone"),
        settings.prefs.language_follows_phone,
        false,
        Act::Language(None),
    )];
    rows.extend(Locale::ALL.into_iter().map(|locale| {
        (
            locale.display_name().to_owned(),
            !settings.prefs.language_follows_phone && settings.locale == locale,
            false,
            Act::Language(Some(locale)),
        )
    }));
    panel::show(
        ctx,
        Panel {
            title: t(&tr, "settings.language"),
            rows,
            chip_selected: -1,
            ..Panel::default()
        },
    );
}

fn open_time_format(ctx: &Ctx) {
    let tr = ctx.tr.borrow();
    let selected = ctx.settings.borrow().prefs.time_format;
    let rows = [TimeFormat::System, TimeFormat::H24, TimeFormat::H12]
        .into_iter()
        .map(|format| {
            (
                format_name(format, &tr),
                selected == format,
                false,
                Act::TimeFormat(format),
            )
        })
        .collect();
    panel::show(
        ctx,
        Panel {
            title: t(&tr, "settings.time_format"),
            rows,
            chip_selected: -1,
            ..Panel::default()
        },
    );
}

fn open_week_start(ctx: &Ctx) {
    let tr = ctx.tr.borrow();
    let selected = ctx.settings.borrow().prefs.week_start;
    let rows = [WeekStart::Monday, WeekStart::Sunday, WeekStart::Saturday]
        .into_iter()
        .map(|start| {
            (
                week_name(start, &tr),
                selected == start,
                false,
                Act::WeekStart(start),
            )
        })
        .collect();
    panel::show(
        ctx,
        Panel {
            title: t(&tr, "settings.week_start"),
            rows,
            chip_selected: -1,
            ..Panel::default()
        },
    );
}

pub fn open_volume(ctx: &Ctx) {
    let tr = ctx.tr.borrow();
    let selected = ctx.settings.borrow().min_volume_percent;
    panel::show(
        ctx,
        Panel {
            title: t(&tr, "settings.min_volume"),
            help: t(&tr, "settings.min_volume_desc"),
            chips: VOLUMES
                .iter()
                .map(|percent| (volume_name(*percent, &tr), Act::Volume(*percent)))
                .collect(),
            chip_selected: VOLUMES
                .iter()
                .position(|percent| *percent == selected)
                .map_or(-1, |i| i as i32),
            done: Some((t(&tr, "action.done"), Act::Close)),
            ..Panel::default()
        },
    );
}

fn open_backup(ctx: &Ctx) {
    let tr = ctx.tr.borrow();
    let mut rows = Vec::new();
    // Own sounds in use give a second choice: the file with them in it is bigger, and the size is told.
    if crate::backup::sounds_used(&ctx.alarms.borrow()).is_empty() {
        rows.push((
            t(&tr, "settings.backup_save"),
            false,
            false,
            Act::BackupSave,
        ));
    } else {
        let size = crate::backup::size_with_sounds(
            &ctx.alarms.borrow(),
            &ctx.settings.borrow().prefs.sound_names,
            &crate::custom_sounds::custom_sounds_dir(),
        );
        rows.push((
            t(&tr, "settings.backup_save_alarms"),
            false,
            false,
            Act::BackupSave,
        ));
        rows.push((
            t(&tr, "settings.backup_save_sounds").replace("{size}", &megabytes(&tr, size)),
            false,
            false,
            Act::BackupSaveWithSounds,
        ));
    }
    rows.push((
        t(&tr, "settings.backup_restore"),
        false,
        false,
        Act::BackupRestore,
    ));
    panel::show(
        ctx,
        Panel {
            title: t(&tr, "settings.backup"),
            help: t(&tr, "settings.backup_note"),
            rows,
            chip_selected: -1,
            ..Panel::default()
        },
    );
}

/// "3.4 MB", at least "0.1 MB".
fn megabytes(tr: &Translations, bytes: u64) -> String {
    let megabytes = (bytes as f64 / 1_000_000.0).max(0.1);
    format!("{} MB", crate::text::one_decimal(tr, megabytes))
}

fn open_about(ctx: &Ctx) {
    let tr = ctx.tr.borrow();
    panel::show(
        ctx,
        Panel {
            title: format!("AlarmFree {}", env!("CARGO_PKG_VERSION")),
            note: t(&tr, "app.description"),
            line: t(&tr, "app.tagline"),
            chip_selected: -1,
            done: Some((t(&tr, "action.done"), Act::Close)),
            about: true,
            ..Panel::default()
        },
    );
}

/// Write the alarms to a file in Downloads, with the own sounds they use if asked. Returns where it went.
fn write_backup(
    alarms: &[AlarmData],
    names: &std::collections::BTreeMap<String, String>,
    with_sounds: bool,
) -> Result<String, crate::platform::SaveError> {
    let name = format!(
        "alarmfree-{}.json",
        crate::clock::local_now().format("%Y-%m-%d")
    );
    let json = crate::backup::export(
        alarms,
        names,
        &crate::custom_sounds::custom_sounds_dir(),
        with_sounds,
    );
    crate::platform::save_to_downloads(&json, &name, "application/json")
}

/// The file with the sounds in it is read and written off the UI thread: it can be a few megabytes.
pub fn save_backup(ctx: &Rc<Ctx>, with_sounds: bool) {
    let alarms = ctx.alarms.borrow().clone();
    let names = ctx.settings.borrow().prefs.sound_names.clone();
    std::thread::spawn(move || {
        let result = write_backup(&alarms, &names, with_sounds);
        let _ = slint::invoke_from_event_loop(move || {
            super::with_ctx(|ctx| backup_saved(ctx, result));
        });
    });
}

fn backup_saved(ctx: &Rc<Ctx>, result: Result<String, crate::platform::SaveError>) {
    let tr = ctx.tr.borrow();
    match result {
        Ok(place) => {
            let message = t(&tr, "settings.backup_saved").replace("{place}", &place);
            drop(tr);
            super::toast::show(ctx, message, None);
        }
        Err(e) => {
            log::warn!("[AlarmFree] Backup failed: {e}");
            let message = t(&tr, e.text_key("settings.backup_failed"));
            drop(tr);
            super::toast::show(ctx, message, None);
        }
    }
}

/// What a restore stored: the alarms as they were before, how many were added, and what could not be
/// brought along.
struct Restore {
    before: Vec<AlarmData>,
    added: usize,
    missing: Vec<String>,
    missing_unnamed: usize,
}

/// Let the user pick a backup and put what is in it on this phone: its sounds, then its alarms. Blocks,
/// so off the UI thread. An empty error is the user changing their mind.
fn read_backup() -> Result<Restore, String> {
    let picked = mobile_sentinel::media_picker::pick_files(
        &["application/json", "text/plain", "application/octet-stream"],
        false,
    )
    .map_err(|_| String::new())?;
    let Some(picked) = picked.into_iter().next() else {
        return Err(String::new());
    };
    let json = std::fs::read_to_string(&picked.path).map_err(|e| e.to_string());
    let _ = std::fs::remove_file(&picked.path);
    let restored = crate::backup::import(
        &json?,
        &crate::custom_sounds::custom_sounds_dir(),
        crate::custom_sounds::sound_exists,
        crate::clock::local_now().naive_local(),
    )
    .ok_or_else(|| "not a backup".to_owned())?;
    for (file, name) in &restored.names {
        crate::persistence::remember_sound_name(file, name);
    }
    let (before, added) = crate::persistence::add_restored_alarms(restored.alarms);
    Ok(Restore {
        before,
        added,
        missing: restored.missing,
        missing_unnamed: restored.missing_unnamed,
    })
}

pub fn restore_backup(_ctx: &Rc<Ctx>) {
    std::thread::spawn(move || {
        // Stored and armed here: the window may be gone when the picker returns.
        let result = read_backup();
        let _ = slint::invoke_from_event_loop(move || {
            super::with_ctx(|ctx| restored(ctx, result));
        });
    });
}

/// The picker is done and the alarms are saved: show them.
fn restored(ctx: &Rc<Ctx>, result: Result<Restore, String>) {
    match result {
        Ok(restore) => {
            *ctx.alarms.borrow_mut() = crate::persistence::load_alarms();
            // The names of the sounds that came along.
            ctx.settings.borrow_mut().prefs.sound_names =
                crate::persistence::load_settings().prefs.sound_names;
            let tr = ctx.tr.borrow();
            let mut message =
                crate::text::counted(&tr, "settings.backup_restored", restore.added as u32);
            if let Some(missing) =
                crate::backup::missing_sounds_text(&tr, &restore.missing, restore.missing_unnamed)
            {
                message = format!("{message}. {missing}");
            }
            drop(tr);
            super::home::refresh(ctx);
            super::toast::show(ctx, message, Some(restore.before));
        }
        Err(e) if e.is_empty() => {}
        Err(e) => {
            log::warn!("[AlarmFree] Restoring failed: {e}");
            let message = t(&ctx.tr.borrow(), "settings.backup_bad");
            super::toast::error(ctx, message);
        }
    }
    panel::close(ctx);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_names_of_the_options_are_the_ones_the_translations_have() {
        let en = i18n::load_translations(Locale::English);
        assert_eq!(volume_name(0, &en), t(&en, "settings.volume_off"));
        assert_eq!(volume_name(75, &en), "75%");
        assert_eq!(week_name(WeekStart::Sunday, &en), t(&en, "day.sunday"));
        assert_eq!(format_name(TimeFormat::H24, &en), t(&en, "settings.h24"));
        assert!(VOLUMES.contains(&AppSettings::default().min_volume_percent));
    }
}
