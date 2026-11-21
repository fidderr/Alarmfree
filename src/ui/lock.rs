//! The edit lock on screen: what is said when a change is refused, and the way to unlock.

use std::rc::Rc;

use chrono::Timelike;
use slint::ComponentHandle;

use super::{Ctx, Ed, EditorSheet};
use crate::edit_lock;
use crate::i18n::t;
use crate::models::{AlarmData, ChallengeSetup};

/// When the locked alarm rings, as a clock time.
fn ring_clock(ctx: &Ctx, at_ms: i64) -> String {
    let at = chrono::DateTime::from_timestamp_millis(at_ms)
        .map(|at| at.with_timezone(&chrono::Local))
        .unwrap_or_else(crate::clock::local_now);
    let twenty_four = crate::text::uses_24_hours(&ctx.settings.borrow());
    crate::text::clock(at.hour() as u8, at.minute() as u8, twenty_four)
}

/// The words for an alarm in a message: its label, or its time.
pub fn alarm_name(ctx: &Ctx, alarm: &AlarmData) -> String {
    let label = alarm.label.trim();
    if label.is_empty() {
        let twenty_four = crate::text::uses_24_hours(&ctx.settings.borrow());
        crate::text::clock(alarm.time_hour, alarm.time_minute, twenty_four)
    } else {
        label.to_owned()
    }
}

/// What the sheet of a locked alarm says.
pub fn help_text(ctx: &Ctx, until_ms: i64) -> String {
    t(&ctx.tr.borrow(), "lock.help").replace("{time}", &ring_clock(ctx, until_ms))
}

fn locked_alarm(ctx: &Ctx, id: &str) -> Option<(AlarmData, i64)> {
    let alarm = ctx.alarms.borrow().iter().find(|a| a.id == id).cloned()?;
    let until = edit_lock::locked_now(&alarm)?;
    Some((alarm, until))
}

/// A change to this alarm from the list: refused while it is locked, with the sheet that offers
/// its challenges. True when refused.
pub fn refuse(ctx: &Rc<Ctx>, id: &str) -> bool {
    let Some((_, until)) = locked_alarm(ctx, id) else {
        return false;
    };
    super::home_sheets::open_locked(ctx, id, until);
    true
}

/// The same from the editor of an alarm that is already saved.
pub fn refuse_in_editor(ctx: &Rc<Ctx>, id: &str) -> bool {
    let Some((_, until)) = locked_alarm(ctx, id) else {
        return false;
    };
    if let Some(ui) = ctx.ui.upgrade() {
        let ed = ui.global::<Ed>();
        ed.set_lock_title(t(&ctx.tr.borrow(), "lock.title").into());
        ed.set_lock_help(help_text(ctx, until).into());
        super::editor::open_sheet(ctx, EditorSheet::Locked);
    }
    true
}

/// A change to every alarm: refused while one is locked, and the message names it.
pub fn refuse_all(ctx: &Rc<Ctx>) -> bool {
    let name = {
        let alarms = ctx.alarms.borrow();
        edit_lock::first_locked(&alarms).map(|alarm| alarm_name(ctx, alarm))
    };
    let Some(name) = name else {
        return false;
    };
    let message = t(&ctx.tr.borrow(), "lock.pause_all").replace("{alarm}", &name);
    super::toast::error(ctx, message);
    true
}

/// "Do its challenges" in either sheet.
pub fn start_unlock(ctx: &Rc<Ctx>, id: &str) {
    let setups = ctx
        .alarms
        .borrow()
        .iter()
        .find(|a| a.id == id)
        .map(|a| ChallengeSetup::list_from_stored(&a.challenges))
        .unwrap_or_default();
    super::challenge_run::start_unlock(ctx, id, &setups);
}

/// The challenges are done: the alarm can be changed for a while.
pub fn unlocked(ctx: &Rc<Ctx>, id: &str) {
    edit_lock::unlock(id, crate::clock::local_now().timestamp_millis());
    let message = t(&ctx.tr.borrow(), "lock.opened")
        .replace("{minutes}", &(edit_lock::OPEN_MS / 60_000).to_string());
    super::toast::show(ctx, message, None);
    super::home::refresh(ctx);
}

/// The Skip on a reminder was tapped for an alarm that is locked: say so.
pub fn say_skips_refused(ctx: &Rc<Ctx>) {
    let names = edit_lock::take_refused_skips();
    if names.is_empty() {
        return;
    }
    let message = t(&ctx.tr.borrow(), "lock.skip_refused").replace("{alarm}", &names.join(", "));
    super::toast::error(ctx, message);
}
