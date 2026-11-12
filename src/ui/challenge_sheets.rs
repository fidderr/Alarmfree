//! What it takes to turn the alarm off, in the editor: the list of challenges, adding one
//! and the settings of one. The rules of the settings are in `crate::challenges::config`.

use std::rc::Rc;

use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

use super::{Cfg, CfgRow, CfgStepper, CfgType, Ctx, EditorSheet, MainWindow};
use crate::challenges::config::{self, DIFFICULTIES, MAX_CHALLENGES};
use crate::i18n::t;
use crate::models::setup::{difficulty_name, CHALLENGE_TYPES};
use crate::models::ChallengeSetup;

fn model<T: Clone + 'static>(items: Vec<T>) -> ModelRc<T> {
    ModelRc::from(Rc::new(VecModel::from(items)))
}

/// Show the list: its rows, the summary of the editor row, and what can be added.
pub fn sync_rows(ctx: &Ctx) {
    let Some(ui) = ctx.ui.upgrade() else {
        return;
    };
    let editor = ctx.editor.borrow();
    let Some(ed) = editor.as_ref() else {
        return;
    };
    let tr = ctx.tr.borrow();
    let cfg = ui.global::<Cfg>();
    cfg.set_summary(config::names_summary(&tr, &ed.setups).into());
    cfg.set_rows(model(
        ed.setups
            .iter()
            .map(|setup| CfgRow {
                name: t(&tr, setup.type_key()).into(),
                summary: config::summary(&tr, setup).into(),
            })
            .collect(),
    ));
    cfg.set_can_add(ed.setups.len() < MAX_CHALLENGES);
    cfg.set_types(model(
        CHALLENGE_TYPES
            .iter()
            .map(|key| CfgType {
                name: t(&tr, key).into(),
                desc: t(&tr, &key.replace("challenge.", "challenge.short.")).into(),
            })
            .collect(),
    ));
    cfg.set_difficulty_labels(model(
        DIFFICULTIES
            .iter()
            .map(|d| SharedString::from(t(&tr, &format!("difficulty.{}", difficulty_name(*d)))))
            .collect(),
    ));
}

/// Show the settings of the challenge being configured.
fn sync_detail(ctx: &Ctx) {
    let Some(ui) = ctx.ui.upgrade() else {
        return;
    };
    let editor = ctx.editor.borrow();
    let Some(ed) = editor.as_ref() else {
        return;
    };
    let Some(setup) = ed.setups.get(ed.detail) else {
        return;
    };
    let tr = ctx.tr.borrow();
    let cfg = ui.global::<Cfg>();
    cfg.set_detail_title(t(&tr, setup.type_key()).into());
    cfg.set_detail_help(
        t(
            &tr,
            &setup.type_key().replace("challenge.", "challenge.desc."),
        )
        .into(),
    );
    cfg.set_detail_example(config::difficulty_example(&tr, setup).into());
    cfg.set_detail_difficulty(
        config::difficulty_of(setup)
            .and_then(|d| DIFFICULTIES.iter().position(|x| *x == d))
            .map_or(-1, |i| i as i32),
    );
    cfg.set_steppers(model(
        config::steppers(setup)
            .into_iter()
            .map(|s| CfgStepper {
                title: t(&tr, s.title_key).into(),
                value: if s.seconds {
                    t(&tr, "text.seconds").replace("{seconds}", &s.value.to_string())
                } else {
                    s.value.to_string()
                }
                .into(),
            })
            .collect(),
    ));
    if let ChallengeSetup::Scan { code } = setup {
        cfg.set_detail_scan(true);
        cfg.set_detail_code(code.clone().into());
    } else {
        cfg.set_detail_scan(false);
    }
}

/// Change the setups, then show them again.
fn edit(ctx: &Ctx, apply: impl FnOnce(&mut Vec<ChallengeSetup>)) {
    if let Some(ed) = ctx.editor.borrow_mut().as_mut() {
        apply(&mut ed.setups);
    }
    sync_rows(ctx);
}

fn setup_at(ctx: &Ctx, index: usize) -> Option<ChallengeSetup> {
    ctx.editor
        .borrow()
        .as_ref()
        .and_then(|ed| ed.setups.get(index).cloned())
}

fn open_detail(ctx: &Ctx, index: usize) {
    if let Some(ed) = ctx.editor.borrow_mut().as_mut() {
        ed.detail = index;
    }
    sync_detail(ctx);
    if let Some(ui) = ctx.ui.upgrade() {
        ui.set_ed_sheet(EditorSheet::Detail);
    }
}

fn back_to_list(ctx: &Ctx) {
    if let Some(ui) = ctx.ui.upgrade() {
        ui.set_ed_sheet(EditorSheet::Challenges);
    }
}

fn detail_index(ctx: &Ctx) -> usize {
    ctx.editor.borrow().as_ref().map_or(0, |ed| ed.detail)
}

fn change_detail(ctx: &Ctx, apply: impl FnOnce(&mut ChallengeSetup)) {
    let index = detail_index(ctx);
    if let Some(setup) = ctx
        .editor
        .borrow_mut()
        .as_mut()
        .and_then(|ed| ed.setups.get_mut(index))
    {
        apply(setup);
    }
    sync_detail(ctx);
    sync_rows(ctx);
}

/// The system's camera scanner, off the UI thread because it waits for the user.
fn scan_into_code(ctx: &Rc<Ctx>) {
    let index = detail_index(ctx);
    std::thread::spawn(move || {
        let value = mobile_sentinel::scanner::scan().unwrap_or_default();
        let _ = slint::invoke_from_event_loop(move || {
            super::with_ctx(|ctx| {
                if value.is_empty() {
                    return;
                }
                if let Some(ChallengeSetup::Scan { code }) = ctx
                    .editor
                    .borrow_mut()
                    .as_mut()
                    .and_then(|ed| ed.setups.get_mut(index))
                {
                    *code = value;
                }
                sync_detail(ctx);
                sync_rows(ctx);
                if let Some(ui) = ctx.ui.upgrade() {
                    let cfg = ui.global::<Cfg>();
                    cfg.set_code_rev(cfg.get_code_rev().wrapping_add(1));
                }
            });
        });
    });
}

pub fn connect(ctx: &Rc<Ctx>, ui: &MainWindow) {
    let cfg = ui.global::<Cfg>();
    macro_rules! on {
        ($setter:ident, |$ctx:ident $(, $arg:ident)*| $body:expr) => {{
            let weak = Rc::downgrade(ctx);
            cfg.$setter(move |$($arg),*| {
                if let Some($ctx) = weak.upgrade() {
                    $body
                }
            });
        }};
    }
    on!(on_open_detail, |ctx, i| open_detail(&ctx, i as usize));
    on!(on_try_one, |ctx, i| {
        if let Some(setup) = setup_at(&ctx, i as usize) {
            super::challenge_run::start_practice(&ctx, &[setup]);
        }
    });
    on!(on_try_all, |ctx| {
        let list = ctx
            .editor
            .borrow()
            .as_ref()
            .map(|ed| ed.setups.clone())
            .unwrap_or_default();
        super::challenge_run::start_practice(&ctx, &list);
    });
    on!(on_remove, |ctx, i| edit(&ctx, |list| {
        if (i as usize) < list.len() {
            list.remove(i as usize);
        }
    }));
    on!(on_move, |ctx, from, to| edit(&ctx, |list| {
        crate::list_edit::move_item(list, from as usize, to as usize);
    }));
    on!(on_add, |ctx, i| {
        let Some(setup) = CHALLENGE_TYPES
            .get(i as usize)
            .and_then(|key| ChallengeSetup::new(key))
        else {
            return;
        };
        if let Some(permission) = config::permission_for(&setup) {
            let _ = mobile_sentinel::permissions::request(permission);
        }
        let mut index = 0;
        edit(&ctx, |list| {
            if list.len() < MAX_CHALLENGES {
                list.push(setup);
            }
            index = list.len().saturating_sub(1);
        });
        open_detail(&ctx, index);
    });
    on!(on_pick_difficulty, |ctx, i| {
        if let Some(picked) = DIFFICULTIES.get(i as usize) {
            change_detail(&ctx, |setup| config::set_difficulty(setup, *picked));
        }
    });
    on!(on_step, |ctx, which, up| change_detail(&ctx, |setup| {
        config::step(setup, which as usize, up)
    }));
    on!(on_code_edited, |ctx, text| {
        // No `sync_detail`: it would replace what is being typed.
        let index = detail_index(&ctx);
        if let Some(ChallengeSetup::Scan { code }) = ctx
            .editor
            .borrow_mut()
            .as_mut()
            .and_then(|ed| ed.setups.get_mut(index))
        {
            *code = text.to_string();
        }
        sync_rows(&ctx);
    });
    on!(on_scan_now, |ctx| scan_into_code(&ctx));
    on!(on_remove_detail, |ctx| {
        let index = detail_index(&ctx);
        edit(&ctx, |list| {
            if index < list.len() {
                list.remove(index);
            }
        });
        back_to_list(&ctx);
    });
    on!(on_try_detail, |ctx| {
        if let Some(setup) = setup_at(&ctx, detail_index(&ctx)) {
            super::challenge_run::start_practice(&ctx, &[setup]);
        }
    });
}
