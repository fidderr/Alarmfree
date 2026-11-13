//! The `ui:` commands of the devtest channel (see `crate::debug_hooks`): what the Slint window
//! shows and a way to press its callbacks by name. Debug builds only.

use std::rc::Rc;

use slint::{ComponentHandle, Model};

use super::{Cal, Cfg, Ctx, Ed, Insets, Look, MainWindow, Motion, Pal, Run, Thm};

fn texts<T: Clone + 'static>(model: impl Model<Data = T>, f: impl Fn(T) -> String) -> Vec<String> {
    model.iter().map(f).collect()
}

fn hex(c: slint::Color) -> String {
    format!("#{:02x}{:02x}{:02x}", c.red(), c.green(), c.blue())
}

/// The main properties of the window as JSON.
fn snapshot(ui: &MainWindow) -> String {
    let rows: Vec<serde_json::Value> = ui
        .get_rows()
        .iter()
        .map(|r| {
            serde_json::json!({
                "id": r.id.as_str(), "time": r.time.as_str(), "label": r.label.as_str(),
                "when": r.when.as_str(), "on": r.on, "can_skip": r.can_skip,
                "skip_active": r.skip_active,
            })
        })
        .collect();
    let value = serde_json::json!({
        "screen": ui.get_screen(),
        "ringing": ui.get_ringing(),
        "home_sheet": format!("{:?}", ui.get_home_sheet()),
        "hs_title": ui.get_hs_title().as_str(),
        "hs_choices": texts(ui.get_hs_choices(), |c| c.text.to_string()),
        "panel_open": ui.get_panel_open(),
        "pn_title": ui.get_pn_title().as_str(),
        "pn_choices": texts(ui.get_pn_choices(), |c| c.text.to_string()),
        "next_title": ui.get_next_title().as_str(),
        "next_sub": ui.get_next_sub().as_str(),
        "paused": ui.get_paused(),
        "banner": ui.get_banner().as_str(),
        "can_reorder": ui.get_can_reorder(),
        "rows": rows,
        "ed_title": ui.get_ed_title().as_str(),
        "ed_time_text": ui.get_ed_time_text().as_str(),
        "ed_rings_text": ui.get_ed_rings_text().as_str(),
        "ed_one_time": ui.get_ed_one_time(),
        "ed_error": ui.get_ed_error().as_str(),
        "ed_sheet": format!("{:?}", ui.get_ed_sheet()),
        "ring_clock": ui.get_ring_clock().as_str(),
        "ring_label": ui.get_ring_label().as_str(),
        "ring_snooze_text": ui.get_ring_snooze_text().as_str(),
        "pal_bg": hex(ui.global::<Pal>().get_bg()),
        "pal_text": hex(ui.global::<Pal>().get_text()),
        "pal_accent": hex(ui.global::<Pal>().get_accent()),
        "pal_light": ui.global::<Pal>().get_light(),
        "less_motion": ui.global::<Motion>().get_less(),
        // The ring screen's look: the backdrop's number in `RingBackdrop::ALL`, its first colour,
        // the clock's number in `RingClock::ALL`, whether a photo is behind it.
        "look_kind": ui.global::<Look>().get_kind(),
        "look_c1": hex(ui.global::<Look>().get_c1()),
        "look_clock": ui.global::<Look>().get_clock_style(),
        "look_photo": ui.global::<Look>().get_has_photo(),
        "ed_preview": ui.get_ed_preview(),
        // How far the system bars and the keyboard cover the window at the bottom, in device pixels.
        "insets_bottom_px": (ui.global::<Insets>().get_bottom() * ui.window().scale_factor()).round(),
        "toast_visible": ui.get_toast_visible(),
        "toast_text": ui.get_toast_text().as_str(),
        "run_active": ui.global::<Run>().get_active(),
        "type_passage": ui.global::<Run>().get_type_cells().iter().map(|c| c.ch.to_string()).collect::<String>(),
        "run_idle_left": ui.global::<Run>().get_idle_left(),
        "scan_state": ui.global::<Run>().get_scan_state(),
        "scanner_open": super::challenge_run::scanner_open(),
    });
    value.to_string()
}

fn screen(ui: &MainWindow) -> String {
    format!(
        "screen={} home_sheet={:?} panel={} editor_sheet={:?} ringing={}",
        ui.get_screen(),
        ui.get_home_sheet(),
        ui.get_panel_open(),
        ui.get_ed_sheet(),
        ui.get_ringing()
    )
}

/// `name` and `arg` of `ui:press:<name>:<arg>`.
fn press(ui: &MainWindow, name: &str, arg: &str) -> String {
    let number = || arg.parse::<i32>().unwrap_or(-1);
    match name {
        "toggle" => ui.invoke_toggle(arg.into()),
        "open" => ui.invoke_open(arg.into()),
        "menu" => ui.invoke_menu(arg.into()),
        "skip" => ui.invoke_skip(arg.into()),
        "delete-alarm" => ui.invoke_delete_alarm(arg.into()),
        "quick" => ui.invoke_quick(number()),
        "create" => ui.invoke_create(),
        "pause-all" => ui.invoke_pause_all(),
        "sort" => ui.invoke_sort(),
        "go" => ui.invoke_go(number()),
        "back" => ui.invoke_back(),
        "hs-picked" => ui.invoke_hs_picked(number()),
        "hs-close" => ui.invoke_hs_close(),
        "pn-picked" => ui.invoke_pn_picked(number()),
        "pn-done-tapped" => ui.invoke_pn_done_tapped(),
        "st-row" => ui.invoke_st_row(number()),
        "ed-back" => ui.invoke_ed_back(),
        "ed-save" => ui.invoke_ed_save(),
        "ed-shift" => ui.invoke_ed_shift(number()),
        "ed-day-toggled" => ui.invoke_ed_day_toggled(number()),
        "ed-preset" => ui.invoke_ed_preset(number()),
        "ed-delete-confirmed" => ui.invoke_ed_delete_confirmed(),
        "ed-discard-confirmed" => ui.invoke_ed_discard_confirmed(),
        "ed-hour" => ui.global::<Ed>().invoke_hour_changed(number()),
        "ed-minute" => ui.global::<Ed>().invoke_minute_changed(number()),
        "ring-snooze" => ui.invoke_ring_snooze(),
        "ring-turn-off" => ui.invoke_ring_turn_off(),
        "toast-undo" => ui.invoke_toast_undo(),
        "h-delete-history" => ui.invoke_h_delete_history(),
        "h-period" => ui.invoke_h_period_picked(number()),
        "h-custom-accepted" => ui.invoke_h_custom_accepted(),
        "toast-dismiss" => ui.invoke_toast_dismiss(),
        "hs-pause-apply" => ui.invoke_hs_pause_apply(),
        "pn-close" => ui.invoke_pn_close(),
        "open-banner" => ui.invoke_open_banner(),
        "hide-banner" => ui.invoke_hide_banner(),
        "template" => ui.invoke_template(number()),
        // Buttons that are only a picture, so no text names them.
        "challenge-remove" => ui.global::<Cfg>().invoke_remove(number()),
        "sound-rename" => ui.global::<Ed>().invoke_rename_start(number()),
        "sound-rename-done" => ui.global::<Ed>().invoke_rename_done(),
        "sound-delete" => ui.global::<Ed>().invoke_delete_ask(number()),
        "sound-delete-yes" => ui.global::<Ed>().invoke_delete_confirm(number()),
        "accent" => ui.global::<Thm>().invoke_accent_picked(number()),
        "theme-tile" => ui.global::<Thm>().invoke_tile_picked(number()),
        "cal-next" => ui.global::<Cal>().invoke_next(),
        "cal-prev" => ui.global::<Cal>().invoke_prev(),
        other => return format!("ERR unknown callback {other}"),
    }
    "OK".to_owned()
}

/// Run one command on the UI thread and give the answer back.
pub fn run(ctx: &Rc<Ctx>, command: &str) -> String {
    let Some(ui) = ctx.ui.upgrade() else {
        return "ERR no window".to_owned();
    };
    #[cfg(feature = "devtest")]
    if let Some(answer) = super::debug_elements::run(&ui, command) {
        return answer;
    }
    match command {
        "screen" => screen(&ui),
        "snapshot" => snapshot(&ui),
        // The scan challenge's button: it needs the run, not just the window.
        "press:scan" => {
            super::challenge_run::scan_tap(ctx);
            "OK".to_owned()
        }
        _ => match command.strip_prefix("press:") {
            Some(rest) => {
                let (name, arg) = rest.split_once(':').unwrap_or((rest, ""));
                press(&ui, name, arg)
            }
            None => format!("ERR unknown command {command}"),
        },
    }
}
