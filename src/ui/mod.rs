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

mod editor;
mod home;
mod home_sheets;
mod palette;
mod ring;
mod toast;

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
    pub home_sheet: RefCell<home_sheets::SheetState>,
    pub preview_timer: slint::Timer,
    pub ring: RefCell<Option<ring::RingState>>,
    pub firing_watch: RefCell<crate::firing_state::FiringWatch>,
    pub ticks: Cell<u32>,
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
    if ui.get_ringing() {
        return;
    }
    if ui.get_screen() == 0 && ui.get_home_sheet() != HomeSheet::None {
        home_sheets::close(ctx);
        return;
    }
    match ui.get_screen() {
        1 => editor::back(ctx),
        _ => mobile_sentinel::foregrounding::move_to_back(),
    }
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
        home_sheet: RefCell::new(home_sheets::SheetState::default()),
        preview_timer: slint::Timer::default(),
        ring: RefCell::new(None),
        firing_watch: RefCell::new(crate::firing_state::FiringWatch::default()),
        ticks: Cell::new(0),
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
    home::refresh(&ctx);

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
    on!(on_quick, |ctx, i| home::quick(&ctx, i as usize));
    on!(on_template, |ctx, i| home::template(&ctx, i as usize));
    on!(on_open, |ctx, id| editor::open(&ctx, id.as_str()));
    on!(on_sort, |ctx| home_sheets::open_sort(&ctx));
    on!(on_pause_all, |ctx| home_sheets::open_pause_all(&ctx));
    on!(on_menu, |ctx, id| home_sheets::open_menu(&ctx, id.as_str()));
    on!(on_hs_close, |ctx| home_sheets::close(&ctx));
    on!(on_hs_picked, |ctx, i| home_sheets::picked(&ctx, i as usize));
    on!(on_hs_pause_step, |ctx, d| home_sheets::pause_step(&ctx, d));
    on!(on_hs_pause_apply, |ctx| home_sheets::pause_apply(&ctx));
    on!(on_create, |ctx| home::create(&ctx));
    on!(on_ed_back, |ctx| editor::back(&ctx));
    on!(on_ed_save, |ctx| editor::save(&ctx));
    on!(on_ed_shift, |ctx, m| editor::shift(&ctx, m));
    on!(on_ed_day_toggled, |ctx, i| editor::toggle_day(
        &ctx, i as usize
    ));
    on!(on_ed_preset, |ctx, i| editor::preset(&ctx, i as usize));
    on!(on_ed_hour_changed, |ctx, h| editor::set_hour(&ctx, h));
    on!(on_ed_minute_changed, |ctx, m| editor::set_minute(&ctx, m));
    on!(on_ed_label_edited, |ctx, s| editor::set_label(
        &ctx,
        s.as_str()
    ));
    on!(on_ed_sound_picked, |ctx, i| editor::pick_sound(
        &ctx, i as usize
    ));
    on!(on_ed_no_sound, |ctx| editor::no_sound(&ctx));
    on!(on_ed_sound_preview, |ctx| editor::toggle_preview(&ctx));
    on!(on_ed_snooze_interval, |ctx, l| editor::snooze_interval(
        &ctx, l
    ));
    on!(on_ed_snooze_count, |ctx, m| editor::snooze_count(&ctx, m));
    on!(on_ed_add_sound, |ctx| editor::add_sound(&ctx));
    on!(on_ed_delete_confirmed, |ctx| editor::delete(&ctx));
    on!(on_ed_discard_confirmed, |ctx| editor::discard(&ctx));
    on!(on_ed_sheet_closed, |ctx| editor::sheet_closed(&ctx));
    on!(on_ring_snooze, |ctx| ring::snooze(&ctx));
    on!(on_ring_turn_off, |ctx| ring::turn_off(&ctx));
    on!(on_toast_undo, |ctx| toast::undo(&ctx));
    on!(on_toast_dismiss, |ctx| toast::dismiss(&ctx));

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
