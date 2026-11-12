//! Doing the challenges one after another: for a ringing alarm and for practice, the same
//! screen and rules. The rules are in `crate::challenges`; this drives their clocks, sensors
//! and the camera, and puts them on the `Run` screen.

use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use slint::{ComponentHandle, Model, ModelRc, VecModel};

use super::{Ctx, MainWindow, ReactTarget, Run, RunCell, RunKind, TypeCell};
use crate::challenges::hold::{Bubble, Finish, HoldState, Press, POP_MS};
use crate::challenges::math::MathState;
use crate::challenges::memory::{self, DisplayStep, MemoryState, Phase, Tap};
use crate::challenges::reaction::{Event as ReactionEvent, ReactionGame, POLL_MS};
use crate::challenges::{
    fraction, idle_left_percent, prepare, typing, Challenge, ChallengeToRun, CELEBRATION_MS,
    FALLBACK_HOLDS, FALLBACK_HOLD_MS, IDLE_LIMIT_SECS, SCAN_QUIET_SECS,
};
use crate::i18n::t;
use crate::models::{ChallengeSetup, ChallengeSolve, ChallengeType};

static SCANNER_OPEN: AtomicBool = AtomicBool::new(false);

/// Whether the camera scanner is on screen right now.
pub fn scanner_open() -> bool {
    SCANNER_OPEN.load(Ordering::Relaxed)
}

/// What the run is for.
pub enum Mode {
    /// A ringing alarm: the way to turn it off.
    Ring(String),
    /// Trying the challenges: nothing rings, nothing is recorded.
    Practice,
}

/// The challenge on screen, with what it keeps between taps and its own clocks.
enum Current {
    Math(MathState),
    Typing {
        passage: String,
        min_accuracy: f32,
        cols: usize,
    },
    Memory {
        state: MemoryState,
        steps: Vec<(u64, DisplayStep)>,
        next: usize,
        timer: slint::Timer,
    },
    /// Shake or steps: counts something and polls for it.
    Counter {
        steps: bool,
        required: u32,
        timer: slint::Timer,
    },
    Hold {
        state: HoldState,
        hold: slint::Timer,
        pop: slint::Timer,
    },
    Reaction {
        game: ReactionGame,
        model: Rc<VecModel<ReactTarget>>,
        timer: slint::Timer,
    },
    Scan {
        expected: String,
        state: i32,
    },
}

pub struct RunState {
    mode: Mode,
    challenges: Vec<ChallengeToRun>,
    index: usize,
    solves: Vec<ChallengeSolve>,
    started_at: chrono::DateTime<chrono::Utc>,
    /// This challenge was swapped for the touch-only one.
    fallback: bool,
    celebrating: bool,
    idle_secs: u32,
    idle_limit: u32,
    scan_quiet: u32,
    scanning_secs: u32,
    last_shake: i32,
    last_steps: i32,
    seq: i32,
    cur: Option<Current>,
    clock: slint::Timer,
    celebration: slint::Timer,
}

fn with_run<R>(ctx: &Ctx, f: impl FnOnce(&mut RunState, &MainWindow) -> R) -> Option<R> {
    let ui = ctx.ui.upgrade()?;
    let mut guard = ctx.run.borrow_mut();
    let run = guard.as_mut()?;
    Some(f(run, &ui))
}

/// A touch or a key: the idle clock starts over.
fn with_input<R>(ctx: &Ctx, f: impl FnOnce(&mut RunState, &MainWindow) -> R) -> Option<R> {
    with_run(ctx, |run, ui| {
        reset_idle(run, ui);
        f(run, ui)
    })
}

fn reset_idle(run: &mut RunState, ui: &MainWindow) {
    run.idle_secs = 0;
    ui.global::<Run>().set_idle_left(100);
}

fn tr(ctx: &Ctx, key: &str) -> String {
    t(&ctx.tr.borrow(), key)
}

fn weak(ctx: &Rc<Ctx>) -> std::rc::Weak<Ctx> {
    Rc::downgrade(ctx)
}

/// The ringing alarm's challenges: turning it off takes these.
pub fn start_ring(ctx: &Rc<Ctx>, id: &str, challenges: Vec<ChallengeToRun>) {
    begin(ctx, Mode::Ring(id.to_owned()), challenges);
}

/// Try challenges without an alarm.
pub fn start_practice(ctx: &Rc<Ctx>, setups: &[ChallengeSetup]) {
    for setup in setups {
        if let Some(permission) = crate::challenges::config::permission_for(setup) {
            let _ = mobile_sentinel::permissions::request(permission);
        }
    }
    let locale = ctx.settings.borrow().locale;
    begin(ctx, Mode::Practice, prepare(setups, locale));
}

fn begin(ctx: &Rc<Ctx>, mode: Mode, challenges: Vec<ChallengeToRun>) {
    abort(ctx);
    let Some(ui) = ctx.ui.upgrade() else {
        return;
    };
    let practice = matches!(mode, Mode::Practice);
    let empty = challenges.is_empty();
    let total = challenges.len();
    let run = RunState {
        mode,
        challenges,
        index: 0,
        solves: Vec::new(),
        started_at: chrono::Utc::now(),
        fallback: false,
        celebrating: false,
        idle_secs: 0,
        idle_limit: crate::platform::shortened("idle_secs", IDLE_LIMIT_SECS),
        scan_quiet: crate::platform::shortened("scan_quiet_secs", SCAN_QUIET_SECS),
        scanning_secs: 0,
        last_shake: mobile_sentinel::sensors::shake_count(),
        last_steps: mobile_sentinel::sensors::step_count(),
        seq: 0,
        cur: None,
        clock: slint::Timer::default(),
        celebration: slint::Timer::default(),
    };
    let screen = ui.global::<Run>();
    screen.set_practice(practice);
    screen.set_finished(empty);
    screen.set_message(tr(ctx, "practice.empty").into());
    screen.set_exit_text(tr(ctx, "practice.exit").into());
    screen.set_total(total as i32);
    screen.set_idle_left(100);
    screen.set_celebrating(false);
    screen.set_asking_fallback(false);
    screen.set_kind(RunKind::None);
    screen.set_active(true);
    *ctx.run.borrow_mut() = Some(run);
    if empty {
        return;
    }
    show_current(ctx);
    if !practice {
        let weak = weak(ctx);
        with_run(ctx, |run, _| {
            run.clock.start(
                slint::TimerMode::Repeated,
                Duration::from_secs(1),
                move || {
                    if let Some(ctx) = weak.upgrade() {
                        clock_tick(&ctx);
                    }
                },
            );
        });
    }
}

/// Stop whatever the current challenge runs: its clocks and its sensors.
fn stop_current(run: &mut RunState) {
    if let Some(Current::Counter { steps, .. }) = &run.cur {
        if *steps {
            mobile_sentinel::sensors::stop_step_counter();
        } else {
            mobile_sentinel::sensors::stop_accelerometer();
        }
    }
    run.cur = None;
}

/// End the run without any outcome: the alarm stopped, or the user left practice.
pub fn abort(ctx: &Rc<Ctx>) {
    let run = ctx.run.borrow_mut().take();
    let Some(mut run) = run else {
        return;
    };
    stop_current(&mut run);
    run.clock.stop();
    run.celebration.stop();
    drop(run);
    if let Some(ui) = ctx.ui.upgrade() {
        let screen = ui.global::<Run>();
        screen.set_active(false);
        screen.set_kind(RunKind::None);
        screen.set_celebrating(false);
        screen.set_asking_fallback(false);
        screen.set_finished(false);
    }
}

pub fn active(ctx: &Ctx) -> bool {
    ctx.run.borrow().is_some()
}

/// The back button while this is up. True when it was taken.
pub fn back(ctx: &Rc<Ctx>) -> bool {
    if !active(ctx) {
        return false;
    }
    let Some(ui) = ctx.ui.upgrade() else {
        return true;
    };
    let screen = ui.global::<Run>();
    if screen.get_asking_fallback() {
        screen.set_asking_fallback(false);
    } else if screen.get_practice() {
        abort(ctx);
    } else if with_run(ctx, |run, _| run.fallback).unwrap_or(false) {
        fallback_cancelled(ctx);
    }
    true
}

pub fn exit(ctx: &Rc<Ctx>) {
    if with_run(ctx, |run, _| matches!(run.mode, Mode::Practice)).unwrap_or(false) {
        abort(ctx);
    }
}

pub fn touched(ctx: &Rc<Ctx>) {
    with_input(ctx, |_, _| ());
}

// ---- The clock of a ringing alarm ---------------------------------------------------------

#[derive(Default)]
struct Tick {
    long_scan: Option<bool>,
    idle: bool,
}

fn clock_tick(ctx: &Rc<Ctx>) {
    let tick = with_run(ctx, |run, ui| {
        let mut tick = Tick::default();
        let screen = ui.global::<Run>();
        if scanner_open() {
            run.idle_secs = 0;
            run.scanning_secs += 1;
            screen.set_idle_left(100);
            if run.scanning_secs == run.scan_quiet {
                tick.long_scan = Some(true);
            }
            return tick;
        }
        if run.scanning_secs >= run.scan_quiet {
            tick.long_scan = Some(false);
        }
        run.scanning_secs = 0;
        let shakes = mobile_sentinel::sensors::shake_count();
        let steps = mobile_sentinel::sensors::step_count();
        if shakes != run.last_shake || steps != run.last_steps {
            run.last_shake = shakes;
            run.last_steps = steps;
            run.idle_secs = 0;
            // A walk is no touch, and the ring limit should know somebody is up.
            if let Some(kit) = crate::platform::alarm_kit_optional() {
                kit.note_activity();
            }
        }
        run.idle_secs += 1;
        screen.set_idle_left(idle_left_percent(run.idle_secs, run.idle_limit) as i32);
        tick.idle = run.idle_secs >= run.idle_limit;
        tick
    });
    let Some(tick) = tick else {
        return;
    };
    let id = ring_id(ctx);
    if let (Some(long), Some(id), Some(kit)) = (
        tick.long_scan,
        id.as_deref(),
        crate::platform::alarm_kit_optional(),
    ) {
        let instance = mobile_sentinel::InstanceId::new(id.to_owned());
        let _ = if long {
            kit.resume(&instance)
        } else {
            kit.pause(&instance)
        };
    }
    if tick.idle {
        // Back to the ringing: AlarmKit brings the sound back.
        abort(ctx);
        if let (Some(id), Some(kit)) = (id, crate::platform::alarm_kit_optional()) {
            let _ = kit.resume(&mobile_sentinel::InstanceId::new(id));
        }
    }
}

fn ring_id(ctx: &Ctx) -> Option<String> {
    ctx.run.borrow().as_ref().and_then(|run| match &run.mode {
        Mode::Ring(id) => Some(id.clone()),
        Mode::Practice => None,
    })
}

// ---- Showing a challenge ------------------------------------------------------------------

fn typing_cols(ui: &MainWindow) -> usize {
    let window = ui.window();
    let width = window.size().width as f32 / window.scale_factor();
    let inner = width.min(560.0) - 40.0 - 28.0;
    ((inner / 9.6).floor() as usize).clamp(10, 60)
}

fn show_current(ctx: &Rc<Ctx>) {
    let Some(ui) = ctx.ui.upgrade() else {
        return;
    };
    let mut guard = ctx.run.borrow_mut();
    let Some(run) = guard.as_mut() else {
        return;
    };
    stop_current(run);
    let screen = ui.global::<Run>();
    let total = run.challenges.len();
    let item = run.challenges[run.index].clone();
    let progress = if matches!(run.mode, Mode::Practice) {
        tr(ctx, "practice.progress")
    } else {
        tr(ctx, "challenge.progress")
    }
    .replace("{current}", &(run.index + 1).to_string())
    .replace("{total}", &total.to_string());
    screen.set_progress(progress.into());
    screen.set_index(run.index as i32);
    screen.set_celebrating(false);
    run.seq += 1;
    screen.set_seq(run.seq);
    let is_hold = matches!(item.challenge, Challenge::Hold { .. });
    screen.set_can_fallback(!run.fallback && !is_hold);
    screen.set_can_cancel(run.fallback);
    let challenge = if run.fallback {
        Challenge::Hold {
            count: FALLBACK_HOLDS,
            hold_ms: FALLBACK_HOLD_MS,
        }
    } else {
        item.challenge
    };
    run.cur = Some(match challenge {
        Challenge::Math {
            expression,
            expected,
        } => {
            screen.set_kind(RunKind::Math);
            screen.set_expression(expression.into());
            screen.set_typed("".into());
            screen.set_wrong(false);
            Current::Math(MathState::new(expected))
        }
        Challenge::Typing {
            passage,
            min_accuracy,
        } => {
            screen.set_kind(RunKind::Typing);
            let cols = typing_cols(&ui);
            show_typing(ctx, &screen, &passage, "", min_accuracy, cols);
            Current::Typing {
                passage,
                min_accuracy,
                cols,
            }
        }
        Challenge::Memory {
            sequence,
            grid_size,
        } => {
            screen.set_kind(RunKind::Memory);
            let (cols, cells) = memory::grid(grid_size);
            screen.set_cols(cols as i32);
            let mut state = MemoryState::new(sequence);
            state.restart();
            let model = Rc::new(VecModel::from(vec![RunCell::default(); usize::from(cells)]));
            screen.set_cells(ModelRc::from(model));
            let steps = state.display_steps();
            let mut current = Current::Memory {
                state,
                steps,
                next: 0,
                timer: slint::Timer::default(),
            };
            show_memory(ctx, &screen, &current);
            schedule_memory(ctx, &mut current);
            current
        }
        Challenge::Shake { count } => start_counter(ctx, &screen, false, count),
        Challenge::Steps { count } => start_counter(ctx, &screen, true, count),
        Challenge::Hold { count, hold_ms } => {
            screen.set_kind(RunKind::Hold);
            let state = HoldState::new(count, hold_ms);
            screen.set_instruction(tr(ctx, "challenge.hold.instruction").into());
            screen.set_hold_ms(state.hold_ms as i32);
            screen.set_hold_state(0);
            screen.set_hold_seq(run.seq);
            screen.set_hold_y(state.pos.0);
            screen.set_hold_x(state.pos.1);
            let current = Current::Hold {
                state,
                hold: slint::Timer::default(),
                pop: slint::Timer::default(),
            };
            show_hold_text(ctx, &screen, &current);
            current
        }
        Challenge::Reaction {
            targets,
            time_ms,
            rounds,
        } => {
            screen.set_kind(RunKind::Reaction);
            screen.set_instruction(tr(ctx, "challenge.reaction.instruction").into());
            let game = ReactionGame::new(targets, time_ms, rounds);
            let model = Rc::new(VecModel::from(vec![
                ReactTarget::default();
                game.slots.len()
            ]));
            screen.set_targets(ModelRc::from(model.clone()));
            show_reaction(ctx, &screen, &game, &model);
            let timer = slint::Timer::default();
            let weak = weak(ctx);
            timer.start(
                slint::TimerMode::Repeated,
                Duration::from_millis(POLL_MS),
                move || {
                    if let Some(ctx) = weak.upgrade() {
                        reaction_tick(&ctx);
                    }
                },
            );
            Current::Reaction { game, model, timer }
        }
        Challenge::Scan { expected } => {
            screen.set_kind(RunKind::Scan);
            screen.set_scan_state(0);
            screen.set_instruction(
                tr(
                    ctx,
                    if expected.is_empty() {
                        "challenge.scan.instruction_any"
                    } else {
                        "challenge.scan.instruction_specific"
                    },
                )
                .into(),
            );
            Current::Scan {
                expected: expected.trim().to_owned(),
                state: 0,
            }
        }
    });
}

// ---- Math ---------------------------------------------------------------------------------

pub fn math_key(ctx: &Rc<Ctx>, key: &str) {
    let Some(digit) = key.chars().next() else {
        return;
    };
    let solved = with_input(ctx, |run, ui| {
        let Some(Current::Math(state)) = &mut run.cur else {
            return false;
        };
        let solved = state.press(digit);
        let screen = ui.global::<Run>();
        screen.set_typed(state.typed.clone().into());
        screen.set_wrong(state.wrong);
        solved
    });
    if solved == Some(true) {
        solved_one(ctx);
    }
}

pub fn math_delete(ctx: &Rc<Ctx>) {
    with_input(ctx, |run, ui| {
        if let Some(Current::Math(state)) = &mut run.cur {
            state.delete();
            let screen = ui.global::<Run>();
            screen.set_typed(state.typed.clone().into());
            screen.set_wrong(false);
        }
    });
}

pub fn math_check(ctx: &Rc<Ctx>) {
    with_input(ctx, |run, ui| {
        if let Some(Current::Math(state)) = &mut run.cur {
            state.check();
            ui.global::<Run>().set_wrong(state.wrong);
        }
    });
}

// ---- Typing -------------------------------------------------------------------------------

fn show_typing(
    ctx: &Ctx,
    screen: &Run<'_>,
    passage: &str,
    typed: &str,
    min_accuracy: f32,
    cols: usize,
) {
    let cells = typing::colour(passage, typed, cols);
    let rows = cells.iter().map(|c| c.row + 1).max().unwrap_or(0);
    screen.set_type_cells(ModelRc::from(Rc::new(VecModel::from(
        cells
            .into_iter()
            .map(|c| TypeCell {
                ch: c.ch.to_string().into(),
                col: c.col as i32,
                row: c.row as i32,
                state: c.state,
            })
            .collect::<Vec<_>>(),
    ))));
    screen.set_type_rows(rows as i32);
    let progress = typing::progress(passage, typed, min_accuracy);
    screen.set_accuracy_text(
        tr(ctx, "challenge.typing.accuracy")
            .replace("{pct}", &progress.accuracy_pct.to_string())
            .into(),
    );
    screen.set_accuracy_good(progress.good);
    screen.set_min_text(
        tr(ctx, "challenge.typing.min_accuracy")
            .replace("{pct}", &progress.min_pct.to_string())
            .into(),
    );
    screen.set_type_progress(
        tr(ctx, "challenge.typing.progress")
            .replace("{typed}", &progress.typed.to_string())
            .replace("{total}", &progress.total.to_string())
            .into(),
    );
    screen.set_wrong(progress.wrong);
}

pub fn typing_edited(ctx: &Rc<Ctx>, text: &str) {
    let done = with_input(ctx, |run, ui| {
        let Some(Current::Typing {
            passage,
            min_accuracy,
            cols,
        }) = &run.cur
        else {
            return false;
        };
        let screen = ui.global::<Run>();
        show_typing(ctx, &screen, passage, text, *min_accuracy, *cols);
        typing::progress(passage, text, *min_accuracy).done
    });
    if done == Some(true) {
        solved_one(ctx);
    }
}

// ---- Memory -------------------------------------------------------------------------------

fn show_memory(ctx: &Ctx, screen: &Run<'_>, current: &Current) {
    let Current::Memory { state, .. } = current else {
        return;
    };
    let model = screen.get_cells();
    let input = state.phase == Phase::Input;
    for cell in 0..model.row_count() as u8 {
        let next = RunCell {
            text: state
                .order_of(cell)
                .map(|n| n.to_string())
                .unwrap_or_default()
                .into(),
            state: state.cell_state(cell),
        };
        if model.row_data(usize::from(cell)).as_ref() != Some(&next) {
            model.set_row_data(usize::from(cell), next);
        }
    }
    screen.set_instruction(
        tr(
            ctx,
            if input {
                "challenge.instruction.memory_input"
            } else {
                "challenge.instruction.memory_watch"
            },
        )
        .into(),
    );
    screen.set_hint(
        tr(
            ctx,
            if input {
                "challenge.memory.hint_input"
            } else {
                "challenge.memory.hint_display"
            },
        )
        .into(),
    );
    let mut status = String::new();
    if input {
        status = tr(ctx, "challenge.memory.progress")
            .replace("{done}", &state.user.len().to_string())
            .replace("{total}", &state.sequence_len().to_string());
        if !state.wrong.is_empty() {
            status += &tr(ctx, "challenge.memory.wrong_count")
                .replace("{wrong}", &state.wrong.len().to_string())
                .replace("{max}", &memory::MAX_WRONG.to_string());
        }
    }
    screen.set_status(status.into());
}

/// Set the clock for the next step of the display phase.
fn schedule_memory(ctx: &Rc<Ctx>, current: &mut Current) {
    let Current::Memory {
        steps, next, timer, ..
    } = current
    else {
        return;
    };
    let Some((delay, _)) = steps.get(*next) else {
        return;
    };
    let weak = weak(ctx);
    timer.start(
        slint::TimerMode::SingleShot,
        Duration::from_millis(*delay),
        move || {
            if let Some(ctx) = weak.upgrade() {
                memory_step(&ctx);
            }
        },
    );
}

fn memory_step(ctx: &Rc<Ctx>) {
    let Some(ui) = ctx.ui.upgrade() else {
        return;
    };
    let mut guard = ctx.run.borrow_mut();
    let Some(run) = guard.as_mut() else {
        return;
    };
    let Some(current) = run.cur.as_mut() else {
        return;
    };
    {
        let Current::Memory {
            state, steps, next, ..
        } = current
        else {
            return;
        };
        if let Some((_, step)) = steps.get(*next).copied() {
            state.apply(step);
            *next += 1;
        }
    }
    show_memory(ctx, &ui.global::<Run>(), current);
    schedule_memory(ctx, current);
}

pub fn memory_tap(ctx: &Rc<Ctx>, cell: i32) {
    let Ok(cell) = u8::try_from(cell) else {
        return;
    };
    let outcome = with_input(ctx, |run, ui| {
        let current = run.cur.as_mut()?;
        let tap = {
            let Current::Memory {
                state,
                steps,
                next,
                timer,
            } = current
            else {
                return None;
            };
            let tap = state.tap(cell);
            if tap == Tap::Replay {
                timer.stop();
                *steps = state.display_steps();
                *next = 0;
            }
            tap
        };
        show_memory(ctx, &ui.global::<Run>(), current);
        if tap == Tap::Replay {
            schedule_memory(ctx, current);
        }
        Some(tap)
    })
    .flatten();
    if outcome == Some(Tap::Complete) {
        solved_one(ctx);
    }
}

// ---- Shake and steps ----------------------------------------------------------------------

fn start_counter(ctx: &Rc<Ctx>, screen: &Run<'_>, steps: bool, required: u32) -> Current {
    screen.set_kind(RunKind::Ring);
    screen.set_instruction(
        tr(
            ctx,
            if steps {
                "challenge.steps.instruction"
            } else {
                "challenge.shake.instruction"
            },
        )
        .into(),
    );
    screen.set_unit(
        tr(
            ctx,
            if steps {
                "challenge.steps.unit"
            } else {
                "challenge.shake.unit"
            },
        )
        .into(),
    );
    screen.set_fraction(0.0);
    screen.set_ring_text(format!("0 / {required}").into());
    // The previous challenge is stopped by now: one that also counts switches the sensor off.
    if steps {
        mobile_sentinel::sensors::start_step_counter();
    } else {
        mobile_sentinel::sensors::reset_shake_count();
        mobile_sentinel::sensors::start_accelerometer();
    }
    let timer = slint::Timer::default();
    let weak = weak(ctx);
    timer.start(
        slint::TimerMode::Repeated,
        Duration::from_millis(if steps { 250 } else { 200 }),
        move || {
            if let Some(ctx) = weak.upgrade() {
                counter_tick(&ctx);
            }
        },
    );
    Current::Counter {
        steps,
        required,
        timer,
    }
}

fn counter_tick(ctx: &Rc<Ctx>) {
    let done = with_run(ctx, |run, ui| {
        let Some(Current::Counter {
            steps,
            required,
            timer,
        }) = &run.cur
        else {
            return false;
        };
        let now = if *steps {
            mobile_sentinel::sensors::step_count()
        } else {
            mobile_sentinel::sensors::shake_count()
        } as u32;
        let screen = ui.global::<Run>();
        screen.set_fraction(fraction(now, *required));
        screen.set_ring_text(format!("{now} / {required}").into());
        if now >= *required {
            timer.stop();
            return true;
        }
        false
    });
    if done == Some(true) {
        solved_one(ctx);
    }
}

// ---- Hold ---------------------------------------------------------------------------------

fn show_hold_text(ctx: &Ctx, screen: &Run<'_>, current: &Current) {
    if let Current::Hold { state, .. } = current {
        screen.set_ring_text(
            format!(
                "{} / {} {}",
                state.completed,
                state.required,
                tr(ctx, "challenge.hold.unit")
            )
            .into(),
        );
    }
}

pub fn hold_down(ctx: &Rc<Ctx>) {
    let started = with_input(ctx, |run, ui| {
        let Some(Current::Hold { state, .. }) = &mut run.cur else {
            return None;
        };
        let Press::Started(token) = state.press() else {
            return None;
        };
        ui.global::<Run>().set_hold_state(1);
        Some((token, state.hold_ms))
    })
    .flatten();
    let Some((token, hold_ms)) = started else {
        return;
    };
    crate::feedback::tick(18);
    // The hold survives this long uninterrupted, or letting go cancels it.
    let weak = weak(ctx);
    with_run(ctx, |run, _| {
        if let Some(Current::Hold { hold, .. }) = &run.cur {
            hold.start(
                slint::TimerMode::SingleShot,
                Duration::from_millis(u64::from(hold_ms)),
                move || {
                    if let Some(ctx) = weak.upgrade() {
                        hold_finished(&ctx, token);
                    }
                },
            );
        }
    });
}

pub fn hold_up(ctx: &Rc<Ctx>) {
    with_input(ctx, |run, ui| {
        if let Some(Current::Hold { state, hold, .. }) = &mut run.cur {
            if state.bubble == Bubble::Filling {
                hold.stop();
                state.release();
                ui.global::<Run>().set_hold_state(0);
            }
        }
    });
}

fn hold_finished(ctx: &Rc<Ctx>, token: u32) {
    let weak = weak(ctx);
    let finish = with_run(ctx, |run, ui| {
        let Some(Current::Hold { state, pop, .. }) = &mut run.cur else {
            return Finish::Stale;
        };
        let finish = state.finish(token);
        if finish == Finish::Stale {
            return finish;
        }
        ui.global::<Run>().set_hold_state(2);
        crate::feedback::tick(if finish == Finish::Last { 130 } else { 45 });
        pop.start(
            slint::TimerMode::SingleShot,
            Duration::from_millis(POP_MS),
            move || {
                if let Some(ctx) = weak.upgrade() {
                    hold_popped(&ctx, finish == Finish::Last);
                }
            },
        );
        finish
    });
    if let Some(finish) = finish {
        if finish != Finish::Stale {
            // The count moves on at the pop.
            with_run(ctx, |run, ui| {
                if let Some(current) = &run.cur {
                    show_hold_text(ctx, &ui.global::<Run>(), current);
                }
            });
        }
    }
}

fn hold_popped(ctx: &Rc<Ctx>, last: bool) {
    if last {
        solved_one(ctx);
        return;
    }
    with_run(ctx, |run, ui| {
        if let Some(Current::Hold { state, .. }) = &mut run.cur {
            state.next_bubble();
            let screen = ui.global::<Run>();
            screen.set_hold_y(state.pos.0);
            screen.set_hold_x(state.pos.1);
            screen.set_hold_state(0);
            // A fresh bubble element, with an empty ring.
            screen.set_hold_seq(screen.get_hold_seq().wrapping_add(1));
        }
    });
}

// ---- Reaction -----------------------------------------------------------------------------

fn show_reaction(
    ctx: &Ctx,
    screen: &Run<'_>,
    game: &ReactionGame,
    model: &Rc<VecModel<ReactTarget>>,
) {
    for (i, slot) in game.slots.iter().enumerate() {
        let next = match slot {
            Some(target) => ReactTarget {
                present: true,
                id: target.id as i32,
                fx: target.fx,
                fy: target.fy,
            },
            None => ReactTarget::default(),
        };
        if model.row_data(i).as_ref() != Some(&next) {
            model.set_row_data(i, next);
        }
    }
    screen.set_counting(game.counting_down());
    screen.set_countdown_secs(game.countdown_secs() as i32);
    screen.set_time_frac(game.time_left());
    screen.set_time_low(!game.counting_down());
    screen.set_ring_text(
        format!(
            "{} / {} \u{00B7} {} / {}",
            game.round,
            game.rounds,
            game.popped,
            game.per_round()
        )
        .into(),
    );
    let _ = ctx;
}

fn reaction_tick(ctx: &Rc<Ctx>) {
    let event = with_run(ctx, |run, ui| {
        let Some(Current::Reaction { game, model, timer }) = &mut run.cur else {
            return ReactionEvent::None;
        };
        let event = game.step(POLL_MS);
        match event {
            ReactionEvent::TimedOut => crate::feedback::tick(60),
            ReactionEvent::RoundCleared => crate::feedback::tick(80),
            ReactionEvent::Solved => {
                crate::feedback::tick(130);
                timer.stop();
            }
            _ => {}
        }
        show_reaction(ctx, &ui.global::<Run>(), game, model);
        event
    });
    if event == Some(ReactionEvent::Solved) {
        solved_one(ctx);
    }
}

pub fn reaction_hit(ctx: &Rc<Ctx>, id: i32) {
    let Ok(id) = u32::try_from(id) else {
        return;
    };
    with_input(ctx, |run, ui| {
        let Some(Current::Reaction { game, model, .. }) = &mut run.cur else {
            return;
        };
        if game.hit(id) {
            crate::feedback::tick(15);
            show_reaction(ctx, &ui.global::<Run>(), game, model);
        }
    });
}

// ---- Scan ---------------------------------------------------------------------------------

pub fn scan_tap(ctx: &Rc<Ctx>) {
    let token = with_input(ctx, |run, ui| {
        let seq = run.seq;
        let Some(Current::Scan { state, .. }) = &mut run.cur else {
            return None;
        };
        if *state != 0 && *state != 2 {
            return None;
        }
        *state = 1;
        ui.global::<Run>().set_scan_state(1);
        Some(seq)
    })
    .flatten();
    let Some(token) = token else {
        return;
    };
    // Blocks until the scanner is done.
    std::thread::spawn(move || {
        SCANNER_OPEN.store(true, Ordering::Relaxed);
        let result = mobile_sentinel::scanner::scan();
        SCANNER_OPEN.store(false, Ordering::Relaxed);
        let scanned = result.unwrap_or_default();
        let _ = slint::invoke_from_event_loop(move || {
            super::with_ctx(|ctx| scan_done(ctx, token, scanned.trim()));
        });
    });
}

fn scan_done(ctx: &Rc<Ctx>, token: i32, scanned: &str) {
    let solved = with_run(ctx, |run, ui| {
        if run.seq != token {
            return false;
        }
        let Some(Current::Scan { expected, state }) = &mut run.cur else {
            return false;
        };
        let screen = ui.global::<Run>();
        // Nothing scanned: the user went back, or it took too long.
        if scanned.is_empty() {
            *state = 0;
        } else if expected.is_empty() || scanned == expected {
            *state = 3;
        } else {
            log::warn!("[AlarmFree] Scanned something else than the expected code");
            *state = 2;
        }
        screen.set_scan_state(*state);
        *state == 3
    });
    if solved == Some(true) {
        solved_one(ctx);
    }
}

// ---- Fallback -----------------------------------------------------------------------------

pub fn fallback_confirmed(ctx: &Rc<Ctx>) {
    let changed = with_input(ctx, |run, _| {
        if run.celebrating {
            return false;
        }
        run.fallback = true;
        true
    });
    if changed == Some(true) {
        show_current(ctx);
    }
}

pub fn fallback_cancelled(ctx: &Rc<Ctx>) {
    let changed = with_input(ctx, |run, _| {
        run.fallback = false;
    });
    if changed.is_some() {
        show_current(ctx);
    }
}

// ---- Solving ------------------------------------------------------------------------------

/// The challenge on screen is done: a moment of celebration, then the next one or the end.
fn solved_one(ctx: &Rc<Ctx>) {
    let confetti = ctx.settings.borrow().prefs.confetti;
    let weak = weak(ctx);
    with_run(ctx, |run, ui| {
        if run.celebrating {
            return;
        }
        let item = &run.challenges[run.index];
        let duration = (chrono::Utc::now() - run.started_at)
            .to_std()
            .unwrap_or_default();
        run.solves.push(ChallengeSolve {
            challenge_type: if run.fallback {
                ChallengeType::HoldButton
            } else {
                item.kind
            },
            difficulty: item.difficulty,
            duration,
        });
        crate::feedback::pattern(&[0, 45, 60, 90]);
        run.celebrating = true;
        let total = run.challenges.len();
        let screen = ui.global::<Run>();
        screen.set_celebration_label(if total > 1 {
            format!("{} / {total}", run.index + 1).into()
        } else {
            "".into()
        });
        screen.set_confetti(if confetti { 28 } else { 0 });
        screen.set_celebrating(true);
        run.celebration.start(
            slint::TimerMode::SingleShot,
            Duration::from_millis(CELEBRATION_MS),
            move || {
                if let Some(ctx) = weak.upgrade() {
                    celebration_over(&ctx);
                }
            },
        );
    });
}

enum Next {
    Another,
    Finished,
}

fn celebration_over(ctx: &Rc<Ctx>) {
    let next = with_run(ctx, |run, _| {
        run.celebrating = false;
        run.fallback = false;
        let next = run.index + 1;
        if next >= run.challenges.len() {
            Next::Finished
        } else {
            run.index = next;
            run.idle_secs = 0;
            run.started_at = chrono::Utc::now();
            Next::Another
        }
    });
    match next {
        Some(Next::Another) => show_current(ctx),
        Some(Next::Finished) => finish(ctx),
        None => {}
    }
}

/// Every challenge is done.
fn finish(ctx: &Rc<Ctx>) {
    let ring = with_run(ctx, |run, ui| {
        stop_current(run);
        match &run.mode {
            Mode::Ring(id) => Some((id.clone(), std::mem::take(&mut run.solves))),
            Mode::Practice => {
                let screen = ui.global::<Run>();
                screen.set_message(tr(ctx, "practice.done").into());
                screen.set_kind(RunKind::None);
                screen.set_celebrating(false);
                screen.set_finished(true);
                None
            }
        }
    })
    .flatten();
    if let Some((id, solves)) = ring {
        abort(ctx);
        super::ring::solved(ctx, &id, solves);
    }
}

/// Hook the screen's callbacks up.
pub fn connect(ctx: &Rc<Ctx>, ui: &MainWindow) {
    let screen = ui.global::<Run>();
    macro_rules! on {
        ($setter:ident, |$ctx:ident $(, $arg:ident)*| $body:expr) => {{
            let weak = Rc::downgrade(ctx);
            screen.$setter(move |$($arg),*| {
                if let Some($ctx) = weak.upgrade() {
                    $body
                }
            });
        }};
    }
    on!(on_touched, |ctx| touched(&ctx));
    on!(on_exit, |ctx| exit(&ctx));
    on!(on_fallback_confirmed, |ctx| fallback_confirmed(&ctx));
    on!(on_fallback_cancelled, |ctx| fallback_cancelled(&ctx));
    on!(on_math_key, |ctx, key| math_key(&ctx, key.as_str()));
    on!(on_math_delete, |ctx| math_delete(&ctx));
    on!(on_math_check, |ctx| math_check(&ctx));
    on!(on_typing_edited, |ctx, text| typing_edited(
        &ctx,
        text.as_str()
    ));
    on!(on_memory_tap, |ctx, cell| memory_tap(&ctx, cell));
    on!(on_hold_down, |ctx| hold_down(&ctx));
    on!(on_hold_up, |ctx| hold_up(&ctx));
    on!(on_reaction_hit, |ctx, id| reaction_hit(&ctx, id));
    on!(on_scan_tap, |ctx| scan_tap(&ctx));
}
