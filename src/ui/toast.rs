//! The message at the bottom of the screen, gone after a while. It stays longer the more it
//! matters and can undo what it reports.

use std::rc::Rc;
use std::time::Duration;

use super::Ctx;
use crate::models::AlarmData;

fn show_with(ctx: &Rc<Ctx>, message: String, error: bool, undo: Option<Vec<AlarmData>>) {
    let Some(ui) = ctx.ui.upgrade() else {
        return;
    };
    let timeout = match (&undo, error) {
        (Some(_), _) => 6000,
        (None, false) => 3000,
        (None, true) => 10000,
    };
    ui.set_toast_text(message.into());
    ui.set_toast_error(error);
    ui.set_toast_has_undo(undo.is_some());
    ui.set_toast_visible(true);
    *ctx.toast_undo.borrow_mut() = undo;
    let weak = Rc::downgrade(ctx);
    ctx.toast_timer.start(
        slint::TimerMode::SingleShot,
        Duration::from_millis(timeout),
        move || {
            if let Some(ctx) = weak.upgrade() {
                dismiss(&ctx);
            }
        },
    );
}

/// A message, with the list as it was if the change can be taken back.
pub fn show(ctx: &Rc<Ctx>, message: String, undo: Option<Vec<AlarmData>>) {
    show_with(ctx, message, false, undo);
}

pub fn error(ctx: &Rc<Ctx>, message: String) {
    show_with(ctx, message, true, None);
}

/// Change the words of the message that is up.
pub fn set_message(ctx: &Rc<Ctx>, message: String) {
    if let Some(ui) = ctx.ui.upgrade() {
        ui.set_toast_text(message.into());
    }
}

pub fn dismiss(ctx: &Rc<Ctx>) {
    ctx.toast_timer.stop();
    ctx.toast_undo.borrow_mut().take();
    if let Some(ui) = ctx.ui.upgrade() {
        ui.set_toast_visible(false);
    }
}

pub fn undo(ctx: &Rc<Ctx>) {
    let before = ctx.toast_undo.borrow_mut().take();
    if let Some(before) = before {
        *ctx.alarms.borrow_mut() = before;
        crate::persistence::save_alarms(&ctx.alarms.borrow());
        super::home::refresh(ctx);
    }
    dismiss(ctx);
}
