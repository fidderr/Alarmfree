//! The settings screen: app-wide preferences, and the panels they open.

use std::rc::Rc;

use slint::ComponentHandle;

use super::panel::{self, Act, Panel};
use super::{Ctx, SettingsView, SCREEN_SETTINGS};
use crate::i18n::{self, t, Translations};
use crate::models::{AlarmData, AppSettings, Locale, TimeFormat, WeekStart};
use crate::{alarm_ops, ordering};

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

/// The animations take no time when the user asked for less motion.
pub fn apply_motion(ctx: &Ctx) {
    if let Some(ui) = ctx.ui.upgrade() {
        // The phone's own "remove animations" option counts as well.
        let less =
            ctx.settings.borrow().prefs.less_motion || mobile_sentinel::display::animations_off();
        ui.global::<super::Motion>().set_less(less);
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
    panel::show(
        ctx,
        Panel {
            title: t(&tr, "settings.backup"),
            note: t(&tr, "settings.backup_note"),
            rows: vec![
                (
                    t(&tr, "settings.backup_save"),
                    false,
                    false,
                    Act::BackupSave,
                ),
                (
                    t(&tr, "settings.backup_restore"),
                    false,
                    false,
                    Act::BackupRestore,
                ),
            ],
            chip_selected: -1,
            ..Panel::default()
        },
    );
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

/// Write the alarms to a file in Downloads. Returns where it went.
fn write_backup(alarms: &[AlarmData]) -> Result<String, String> {
    let name = format!("alarmfree-{}.json", chrono::Local::now().format("%Y-%m-%d"));
    crate::platform::save_to_downloads(&alarm_ops::export_json(alarms), &name, "application/json")
}

pub fn save_backup(ctx: &Rc<Ctx>) {
    let result = write_backup(&ctx.alarms.borrow());
    let tr = ctx.tr.borrow();
    match result {
        Ok(place) => {
            let message = t(&tr, "settings.backup_saved").replace("{place}", &place);
            drop(tr);
            super::toast::show(ctx, message, None);
        }
        Err(e) => {
            log::warn!("[AlarmFree] Backup failed: {e}");
            let message = t(&tr, "settings.backup_failed");
            drop(tr);
            super::toast::show(ctx, message, None);
        }
    }
}

/// Let the user pick a backup and read the alarms from it. Blocks, so off the UI
/// thread. An empty error is the user changing their mind.
fn read_backup() -> Result<Vec<AlarmData>, String> {
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
    alarm_ops::import_json(
        &json?,
        crate::custom_sounds::sound_exists,
        chrono::Local::now().naive_local(),
    )
    .ok_or_else(|| "not a backup".to_owned())
}

pub fn restore_backup(_ctx: &Rc<Ctx>) {
    std::thread::spawn(move || {
        let result = read_backup();
        let _ = slint::invoke_from_event_loop(move || {
            super::with_ctx(|ctx| restored(ctx, result));
        });
    });
}

/// The picker is done: add what it found, as far as there is room.
fn restored(ctx: &Rc<Ctx>, result: Result<Vec<AlarmData>, String>) {
    match result {
        Ok(found) => {
            let before = ctx.alarms.borrow().clone();
            let room = alarm_ops::MAX_ALARMS.saturating_sub(before.len());
            let added = found.len().min(room);
            {
                let mut list = ctx.alarms.borrow_mut();
                let first = ordering::order_for_new(&list);
                for (position, mut alarm) in found.into_iter().take(room).enumerate() {
                    alarm.order = first + position as i64;
                    list.push(alarm);
                }
            }
            crate::persistence::save_alarms(&ctx.alarms.borrow());
            let tr = ctx.tr.borrow();
            let message = crate::text::counted(&tr, "settings.backup_restored", added as u32);
            drop(tr);
            super::home::refresh(ctx);
            super::toast::show(ctx, message, Some(before));
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
