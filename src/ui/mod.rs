//! The UI: Slint, drawn on the GPU, in a NativeActivity on Android and a window on a desktop.
//! The `.slint` files are in `ui/`; this is the Rust side that feeds them and takes their
//! callbacks.

// Clippy asks for a `const` initializer on the Android target, which the slot already has.
#![allow(clippy::missing_const_for_thread_local)]

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use slint::ComponentHandle;

use crate::i18n::{self, Translations};
use crate::models::{AlarmData, AppSettings};

slint::include_modules!();

mod challenge_run;
mod challenge_sheets;
mod colour;
mod datepicker;
mod editor;
mod editor_look;
mod editor_sound;
mod history;
mod home;
mod home_sheets;
mod look;
mod palette;
mod panel;
mod reliability;
mod ring;
mod settings;
mod theme;
mod toast;

/// The screens of `MainWindow.screen`.
const SCREEN_HISTORY: i32 = 2;
const SCREEN_SETTINGS: i32 = 3;
const SCREEN_RELIABILITY: i32 = 4;
const SCREEN_THEME: i32 = 5;

/// Everything the screens share. Only the UI thread touches it.
pub struct Ctx {
    pub ui: slint::Weak<MainWindow>,
    pub settings: RefCell<AppSettings>,
    pub alarms: RefCell<Vec<AlarmData>>,
    pub tr: RefCell<Translations>,
    pub system_dark: Cell<bool>,
    pub toast_undo: RefCell<Option<Vec<AlarmData>>>,
    pub toast_timer: slint::Timer,
    pub timers: RefCell<Vec<slint::Timer>>,
    pub editor: RefCell<Option<editor::EditorState>>,
    /// The alarms the editor was showing before another one was picked next to the list.
    pub editor_trail: RefCell<Vec<String>>,
    pub home_sheet: RefCell<home_sheets::SheetState>,
    pub panel: RefCell<panel::PanelState>,
    pub history: history::HistoryState,
    pub preview_timer: slint::Timer,
    pub ring: RefCell<Option<ring::RingState>>,
    pub run: RefCell<Option<challenge_run::RunState>>,
    pub firing_watch: RefCell<crate::firing_state::FiringWatch>,
    pub ticks: Cell<u32>,
    /// The most serious thing the phone does not allow, shown as a banner on Home.
    pub problem: Cell<Option<mobile_sentinel::health::Check>>,
    pub reliability: RefCell<reliability::State>,
    pub theme: RefCell<theme::State>,
}

// For what comes back to the UI thread from another one.
thread_local! {
    static CTX: RefCell<std::rc::Weak<Ctx>> = const { RefCell::new(std::rc::Weak::new()) };
}

/// Run `f` with the shared state, on the UI thread.
fn with_ctx(f: impl FnOnce(&Rc<Ctx>)) {
    if let Some(ctx) = CTX.with(|ctx| ctx.borrow().upgrade()) {
        f(&ctx);
    }
}

/// History is kept this long.
const HISTORY_KEEP_DAYS: u32 = 5 * 365;

/// From this width on: a tablet, or a foldable when unfolded.
pub const SPLIT_FROM_WIDTH: f32 = 840.0;

/// The system bars take the colour of the page, and their icons contrast with it.
fn set_system_bars(palette: &crate::models::Palette) {
    mobile_sentinel::display::set_light_system_bars(palette.light);
    mobile_sentinel::display::set_system_bar_color(palette.background_argb());
}

/// Apply the theme to the window and the system bars.
fn apply_theme(ctx: &Ctx) {
    let Some(ui) = ctx.ui.upgrade() else {
        return;
    };
    let palette = ctx.settings.borrow().palette(ctx.system_dark.get());
    palette::apply(&ui, &palette);
    set_system_bars(&palette);
}

/// The colour of the window before the UI has painted anything.
pub fn start_background() -> (u8, u8, u8, u8) {
    let settings = crate::persistence::load_settings();
    let rgb = settings
        .palette(mobile_sentinel::display::system_dark())
        .background;
    (rgb.r, rgb.g, rgb.b, 255)
}

/// Go back one step: whatever is on top takes it, else the screen is left. While an alarm
/// rings there is nothing to go back to.
fn back(ctx: &Rc<Ctx>) {
    let Some(ui) = ctx.ui.upgrade() else {
        return;
    };
    if challenge_run::back(ctx) {
        return;
    }
    if ui.get_ringing() {
        return;
    }
    if ui.get_screen() == 0 && ui.get_home_sheet() != HomeSheet::None {
        home_sheets::close(ctx);
        return;
    }
    if ui.get_panel_open() {
        panel::close(ctx);
        return;
    }
    if ui.get_ed_preview() {
        ui.set_ed_preview(false);
        return;
    }
    match ui.get_screen() {
        1 => editor::back(ctx),
        SCREEN_RELIABILITY => reliability::leave(ctx),
        SCREEN_THEME => theme::leave(ctx),
        // The main screens do not stack: back from them is the alarms.
        SCREEN_HISTORY | SCREEN_SETTINGS => go(ctx, 0),
        _ => mobile_sentinel::foregrounding::move_to_back(),
    }
}

/// The bottom navigation: 0 alarms, 1 history, 2 settings.
fn go(ctx: &Rc<Ctx>, index: i32) {
    let Some(ui) = ctx.ui.upgrade() else {
        return;
    };
    match index {
        1 => history::enter(ctx),
        2 => settings::enter(ctx),
        _ => {
            ui.set_panel_open(false);
            ui.set_screen(0);
            home::refresh(ctx);
        }
    }
}

/// The callbacks of the editor's sheets.
fn connect_editor_sheets(ctx: &Rc<Ctx>, ui: &MainWindow) {
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
    on!(Ed, on_hour_changed, |ctx, h| editor::set_hour(&ctx, h));
    on!(Ed, on_minute_changed, |ctx, m| editor::set_minute(&ctx, m));
    on!(Ed, on_time_typed, |ctx, s| editor::time_typed(
        &ctx,
        s.as_str()
    ));
    on!(Ed, on_date_quick, |ctx, i| editor::date_quick(
        &ctx, i as usize
    ));
    on!(Ed, on_label_edited, |ctx, s| editor::set_label(
        &ctx,
        s.as_str()
    ));
    on!(Ed, on_note_edited, |ctx, s| editor::set_note(
        &ctx,
        s.as_str()
    ));
    on!(Ed, on_used_picked, |ctx, i| editor::use_label(
        &ctx, i as usize
    ));
    on!(Ed, on_bright_toggled, |ctx, on| editor::set_max_brightness(
        &ctx, on
    ));
    on!(Ed, on_sound_picked, |ctx, i| editor_sound::pick_sound(
        &ctx, i as usize
    ));
    on!(Ed, on_no_sound, |ctx| editor_sound::no_sound(&ctx));
    on!(Ed, on_sound_preview, |ctx| editor_sound::toggle_preview(
        &ctx
    ));
    on!(Ed, on_add_sound, |ctx| editor_sound::add_sound(&ctx));
    on!(Ed, on_rename_start, |ctx, i| editor_sound::rename_start(
        &ctx, i as usize
    ));
    on!(Ed, on_rename_edited, |ctx, s| editor_sound::rename_edited(
        &ctx,
        s.as_str()
    ));
    on!(Ed, on_rename_done, |ctx| editor_sound::rename_done(&ctx));
    on!(Ed, on_delete_ask, |ctx, i| editor_sound::delete_ask(
        &ctx, i as usize
    ));
    on!(
        Ed,
        on_delete_confirm,
        |ctx, i| editor_sound::delete_confirm(&ctx, i as usize)
    );
    on!(Ed, on_delete_cancel, |ctx| editor_sound::delete_cancel(
        &ctx
    ));
    on!(Ed, on_volume_changed, |ctx, v| editor_sound::set_volume(
        &ctx, v
    ));
    on!(Ed, on_fade_picked, |ctx, i| editor_sound::pick_fade(
        &ctx, i as usize
    ));
    on!(Ed, on_snooze_interval, |ctx, l| editor::snooze_interval(
        &ctx, l
    ));
    on!(Ed, on_snooze_count, |ctx, m| editor::snooze_count(&ctx, m));
    on!(Ed, on_vib_picked, |ctx, i| editor_sound::vib_picked(
        &ctx, i as usize
    ));
    on!(Ed, on_vib_preview, |ctx| editor_sound::vib_preview(&ctx));
    on!(Ed, on_delay_picked, |ctx, i| editor_sound::delay_picked(
        &ctx, i as usize
    ));
    on!(Ed, on_opt_picked, |ctx, i| editor_sound::opt_picked(
        &ctx, i as usize
    ));
    on!(Ed, on_tile_picked, |ctx, i| editor_look::tile_picked(
        &ctx, i as usize
    ));
    on!(Ed, on_photo_other, |ctx| editor_look::pick_photo(&ctx));
    on!(Ed, on_colour_open, |ctx| editor_look::colour_open(&ctx));
    on!(Ed, on_clock_picked, |ctx, i| editor_look::clock_picked(
        &ctx, i as usize
    ));
    on!(Ed, on_look_preview, |ctx| editor_look::preview(&ctx));
    on!(Ed, on_swatch_picked, |ctx, i| editor_look::swatch_picked(
        &ctx, i as usize
    ));
    on!(Cal, on_prev, |ctx| datepicker::route(
        &ctx,
        datepicker::Move::Prev
    ));
    on!(Cal, on_next, |ctx| datepicker::route(
        &ctx,
        datepicker::Move::Next
    ));
    on!(Cal, on_picked, |ctx, i| datepicker::route(
        &ctx,
        datepicker::Move::Pick(i as usize)
    ));
    colour::connect(ctx, ui);
    reliability::connect(ctx, ui);
    theme::connect(ctx, ui);
}

pub fn run() -> Result<(), slint::PlatformError> {
    // First, so the alarm list below already reflects one-time alarms it switched off.
    crate::platform::init_platform();

    let settings = crate::persistence::load_settings();
    let alarms = crate::persistence::load_alarms();
    crate::feedback::set_enabled(settings.prefs.touch_feedback);
    crate::history_persistence::delete_history(Some(HISTORY_KEEP_DAYS));
    crate::ring_photos::prune(&alarms, settings.prefs.alarm_defaults.as_ref());

    let ui = MainWindow::new()?;
    let ctx = Rc::new(Ctx {
        ui: ui.as_weak(),
        tr: RefCell::new(i18n::load_translations(settings.locale)),
        settings: RefCell::new(settings),
        alarms: RefCell::new(alarms),
        system_dark: Cell::new(mobile_sentinel::display::system_dark()),
        toast_undo: RefCell::new(None),
        toast_timer: slint::Timer::default(),
        timers: RefCell::new(Vec::new()),
        editor: RefCell::new(None),
        editor_trail: RefCell::new(Vec::new()),
        home_sheet: RefCell::new(home_sheets::SheetState::default()),
        panel: RefCell::new(panel::PanelState::default()),
        history: history::HistoryState::default(),
        preview_timer: slint::Timer::default(),
        ring: RefCell::new(None),
        run: RefCell::new(None),
        firing_watch: RefCell::new(crate::firing_state::FiringWatch::default()),
        ticks: Cell::new(0),
        problem: Cell::new(None),
        reliability: RefCell::new(reliability::State::default()),
        theme: RefCell::new(theme::State::default()),
    });
    CTX.with(|slot| *slot.borrow_mut() = Rc::downgrade(&ctx));

    {
        let ctx = Rc::downgrade(&ctx);
        ui.global::<Tr>().on_lookup(move |key, _| {
            ctx.upgrade()
                .map(|ctx| i18n::t(&ctx.tr.borrow(), key.as_str()))
                .unwrap_or_else(|| key.to_string())
                .into()
        });
    }
    apply_theme(&ctx);
    settings::apply_motion(&ctx);
    reliability::refresh_banner(&ctx);
    home::refresh(&ctx);
    let first_run = !ctx.settings.borrow().prefs.setup_done;

    macro_rules! on {
        ($setter:ident, |$ctx:ident $(, $arg:ident)*| $body:expr) => {{
            let weak = Rc::downgrade(&ctx);
            ui.$setter(move |$($arg),*| {
                if let Some($ctx) = weak.upgrade() {
                    $body
                }
            });
        }};
    }
    on!(on_toggle, |ctx, id| home::toggle(&ctx, id.as_str()));
    on!(on_skip, |ctx, id| home::skip_from_swipe(&ctx, id.as_str()));
    on!(on_delete_alarm, |ctx, id| home::delete_alarm(
        &ctx,
        id.as_str()
    ));
    on!(on_reorder, |ctx, from, to| home::reorder(
        &ctx,
        from as usize,
        to as usize
    ));
    on!(on_reorder_started, |_ctx| crate::feedback::tick(15));
    on!(on_quick, |ctx, i| home::quick(&ctx, i as usize));
    on!(on_template, |ctx, i| home::template(&ctx, i as usize));
    on!(on_open, |ctx, id| editor::open(&ctx, id.as_str()));
    on!(on_sort, |ctx| home_sheets::open_sort(&ctx));
    on!(on_pause_all, |ctx| home_sheets::open_pause_all(&ctx));
    on!(on_menu, |ctx, id| home_sheets::open_menu(&ctx, id.as_str()));
    on!(on_hs_close, |ctx| home_sheets::close(&ctx));
    on!(on_hs_picked, |ctx, i| home_sheets::picked(&ctx, i as usize));
    on!(on_hs_pause_apply, |ctx| home_sheets::pause_apply(&ctx));
    on!(on_create, |ctx| home::create(&ctx));
    on!(on_open_banner, |ctx| reliability::open(&ctx, false));
    on!(on_hide_banner, |ctx| reliability::hide_banner(&ctx));
    on!(on_go, |ctx, i| go(&ctx, i));
    on!(on_st_row, |ctx, i| settings::row(&ctx, i));
    on!(on_st_toggled, |ctx, i, on| settings::toggled(&ctx, i, on));
    on!(on_h_delete_history, |ctx| history::open_delete(&ctx));
    on!(on_h_period_picked, |ctx, i| history::period_picked(
        &ctx, i as usize
    ));
    on!(on_h_custom_edited, |ctx, s| history::custom_edited(
        &ctx,
        s.as_str()
    ));
    on!(on_h_custom_accepted, |ctx| history::custom_accepted(&ctx));
    on!(on_h_delete_event, |ctx, id| history::ask_delete_event(
        &ctx,
        id.as_str()
    ));
    on!(on_pn_picked, |ctx, i| panel::picked(&ctx, i as usize));
    on!(on_pn_chip_picked, |ctx, i| panel::chip_picked(
        &ctx, i as usize
    ));
    on!(on_pn_done_tapped, |ctx| panel::done(&ctx));
    on!(on_ed_back, |ctx| editor::back(&ctx));
    on!(on_ed_save, |ctx| editor::save(&ctx));
    on!(on_ed_shift, |ctx, m| editor::shift(&ctx, m));
    on!(on_ed_day_toggled, |ctx, i| editor::toggle_day(
        &ctx, i as usize
    ));
    on!(on_ed_preset, |ctx, i| editor::preset(&ctx, i as usize));
    on!(on_ed_sheet_opened, |ctx, sheet| editor::sheet_opened(
        &ctx, sheet
    ));
    connect_editor_sheets(&ctx, &ui);
    on!(on_ed_delete_confirmed, |ctx| editor::delete(&ctx));
    on!(on_ed_discard_confirmed, |ctx| editor::discard(&ctx));
    on!(on_ed_sheet_closed, |ctx| editor::sheet_closed(&ctx));
    on!(on_ring_snooze, |ctx| ring::snooze(&ctx));
    on!(on_ring_turn_off, |ctx| ring::turn_off(&ctx));
    challenge_run::connect(&ctx, &ui);
    challenge_sheets::connect(&ctx, &ui);
    on!(on_toast_undo, |ctx| toast::undo(&ctx));
    on!(on_toast_dismiss, |ctx| toast::dismiss(&ctx));

    // The setup comes before anything else, once.
    if first_run {
        reliability::open(&ctx, true);
    }

    // Anything the user must know about what happened while the app was away: a database that
    // had to be reset (or cannot persist) instead of a silently empty alarm list, and alarms
    // that ended without the user.
    {
        use crate::persistence::DbRecovery;
        let mut notices = Vec::new();
        {
            let tr = ctx.tr.borrow();
            match crate::persistence::db_recovery() {
                DbRecovery::None => {}
                DbRecovery::Recovered { .. } => notices.push(i18n::t(&tr, "db.recovered")),
                DbRecovery::Volatile => notices.push(i18n::t(&tr, "db.volatile")),
            }
            let unreadable = crate::persistence::unreadable_alarms();
            if unreadable > 0 {
                notices.push(
                    i18n::t(&tr, "db.unreadable").replace("{count}", &unreadable.to_string()),
                );
            }
        }
        notices.extend(ring::unseen_outcome_notices(&ctx));
        if !notices.is_empty() {
            toast::error(&ctx, notices.join(" "));
        }
    }

    // Whether an alarm rings: primed from disk at once, then polled, because the ring state
    // machine writes it from another process.
    ring::poll(&ctx);
    {
        let weak = Rc::downgrade(&ctx);
        let poll = slint::Timer::default();
        poll.start(
            slint::TimerMode::Repeated,
            Duration::from_millis(crate::firing_state::POLL_MS),
            move || {
                if let Some(ctx) = weak.upgrade() {
                    ring::poll(&ctx);
                }
            },
        );
        ctx.timers.borrow_mut().push(poll);
    }

    // What the clock moves: the countdown, "Today" and "Tomorrow". Soon after coming up too,
    // because what the startup found may only just be in.
    {
        let weak = Rc::downgrade(&ctx);
        let timer = slint::Timer::default();
        timer.start(
            slint::TimerMode::Repeated,
            Duration::from_secs(15),
            move || {
                if let Some(ctx) = weak.upgrade() {
                    reliability::refresh_banner(&ctx);
                    home::refresh(&ctx);
                }
            },
        );
        let weak = Rc::downgrade(&ctx);
        let soon = slint::Timer::default();
        soon.start(
            slint::TimerMode::SingleShot,
            Duration::from_millis(1500),
            move || {
                if let Some(ctx) = weak.upgrade() {
                    reliability::refresh_banner(&ctx);
                    home::refresh(&ctx);
                }
            },
        );
        // The phone's dark mode, and a window that was recreated with other bar colours.
        let weak = Rc::downgrade(&ctx);
        let theme = slint::Timer::default();
        theme.start(
            slint::TimerMode::Repeated,
            Duration::from_secs(2),
            move || {
                if let Some(ctx) = weak.upgrade() {
                    let dark = mobile_sentinel::display::system_dark();
                    if dark != ctx.system_dark.get() {
                        ctx.system_dark.set(dark);
                        apply_theme(&ctx);
                    }
                }
            },
        );
        // The bars may settle just after the start, and a recreated window can reset them.
        let mut timers = vec![timer, soon, theme];
        for delay_ms in [300, 1500] {
            let weak = Rc::downgrade(&ctx);
            let once = slint::Timer::default();
            once.start(
                slint::TimerMode::SingleShot,
                Duration::from_millis(delay_ms),
                move || {
                    if let Some(ctx) = weak.upgrade() {
                        apply_theme(&ctx);
                    }
                },
            );
            timers.push(once);
        }
        ctx.timers.borrow_mut().extend(timers);
    }

    // Back: delivered on Android's main thread, handled on the UI thread.
    {
        // `Rc` is not `Send`: the callback only carries the window handle across.
        let ui_weak = ui.as_weak();
        mobile_sentinel::foregrounding::on_back(move || {
            let ui_weak = ui_weak.clone();
            let _ = slint::invoke_from_event_loop(move || {
                if let Some(ui) = ui_weak.upgrade() {
                    ui.invoke_back();
                }
            });
        });
        on!(on_back, |ctx| back(&ctx));
    }

    ui.run()
}
