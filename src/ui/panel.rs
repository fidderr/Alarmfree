//! The panel over the history and the settings: what it shows and what a tap in it does.

use std::rc::Rc;

use slint::{ModelRc, SharedString, VecModel};

use super::{Ctx, SheetChoice};
use crate::models::{Locale, TimeFormat, WeekStart};

/// What a row, a chip or the Done button of the open panel does.
#[derive(Clone)]
pub enum Act {
    Language(Option<Locale>),
    TimeFormat(TimeFormat),
    WeekStart(WeekStart),
    Volume(u8),
    BackupSave,
    BackupRestore,
    /// Ask once more before this much history goes: older than so many days, or all of it.
    AskDelete(Option<u32>),
    DeleteHistory(Option<u32>),
    DeleteEvent(String),
    Close,
}

#[derive(Default)]
pub struct PanelState {
    rows: Vec<Act>,
    chips: Vec<Act>,
    done: Option<Act>,
}

/// What the panel says.
#[derive(Default)]
pub struct Panel {
    pub title: String,
    pub help: String,
    pub note: String,
    pub line: String,
    /// Text, selected, danger, and what it does.
    pub rows: Vec<(String, bool, bool, Act)>,
    pub chips: Vec<(String, Act)>,
    pub chip_selected: i32,
    pub done: Option<(String, Act)>,
    /// Shows Slint's attribution (the About panel).
    pub about: bool,
}

pub fn show(ctx: &Ctx, panel: Panel) {
    let Some(ui) = ctx.ui.upgrade() else {
        return;
    };
    let choices: Vec<SheetChoice> = panel
        .rows
        .iter()
        .map(|(text, selected, danger, _)| SheetChoice {
            text: text.clone().into(),
            selected: *selected,
            danger: *danger,
        })
        .collect();
    let chip_labels: Vec<SharedString> = panel
        .chips
        .iter()
        .map(|(label, _)| label.clone().into())
        .collect();
    let (done_text, done) = match panel.done {
        Some((text, act)) => (text, Some(act)),
        None => (String::new(), None),
    };
    *ctx.panel.borrow_mut() = PanelState {
        rows: panel.rows.into_iter().map(|(.., act)| act).collect(),
        chips: panel.chips.into_iter().map(|(_, act)| act).collect(),
        done,
    };
    ui.set_pn_title(panel.title.into());
    ui.set_pn_help(panel.help.into());
    ui.set_pn_note(panel.note.into());
    ui.set_pn_line(panel.line.into());
    ui.set_pn_done(done_text.into());
    ui.set_pn_choices(ModelRc::from(Rc::new(VecModel::from(choices))));
    ui.set_pn_chips(ModelRc::from(Rc::new(VecModel::from(chip_labels))));
    ui.set_pn_chip_selected(panel.chip_selected);
    ui.set_pn_about(panel.about);
    ui.set_panel_open(true);
}

pub fn close(ctx: &Ctx) {
    if let Some(ui) = ctx.ui.upgrade() {
        ui.set_panel_open(false);
    }
}

pub fn picked(ctx: &Rc<Ctx>, index: usize) {
    let act = ctx.panel.borrow().rows.get(index).cloned();
    if let Some(act) = act {
        run(ctx, act);
    }
}

pub fn chip_picked(ctx: &Rc<Ctx>, index: usize) {
    let act = ctx.panel.borrow().chips.get(index).cloned();
    if let Some(act) = act {
        run(ctx, act);
    }
}

pub fn done(ctx: &Rc<Ctx>) {
    let act = ctx.panel.borrow().done.clone();
    run(ctx, act.unwrap_or(Act::Close));
}

fn run(ctx: &Rc<Ctx>, act: Act) {
    match act {
        Act::Language(locale) => {
            super::settings::set_language(ctx, locale);
            close(ctx);
        }
        Act::TimeFormat(format) => {
            super::settings::update(ctx, |s| s.prefs.time_format = format);
            close(ctx);
        }
        Act::WeekStart(start) => {
            super::settings::update(ctx, |s| s.prefs.week_start = start);
            close(ctx);
        }
        Act::Volume(percent) => {
            super::settings::update(ctx, |s| s.min_volume_percent = percent);
            crate::platform::set_min_volume(percent);
            super::settings::open_volume(ctx);
        }
        Act::BackupSave => {
            super::settings::save_backup(ctx);
            close(ctx);
        }
        // The panel stays up until the picker is done.
        Act::BackupRestore => super::settings::restore_backup(ctx),
        Act::AskDelete(days) => super::history::ask_delete(ctx, days),
        Act::DeleteHistory(days) => {
            super::history::delete_history(ctx, days);
            close(ctx);
        }
        Act::DeleteEvent(id) => {
            super::history::delete_event(ctx, &id);
            close(ctx);
        }
        Act::Close => close(ctx),
    }
}
