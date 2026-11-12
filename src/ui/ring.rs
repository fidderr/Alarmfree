//! The screen while an alarm rings. A pure view over the firing state: audio, lock-down,
//! wake lock and notification are mobile-sentinel's, through AlarmKit. The UI only calls its
//! lifecycle methods: `snooze` when the user snoozes, `dismiss` when the alarm is over.

use std::rc::Rc;

use chrono::{Datelike, Timelike};

use super::Ctx;
use crate::i18n::t;
use crate::models::{AlarmData, AlarmEvent, AlarmId, AlarmOutcome, ChallengeSetup, ChallengeSolve};

/// How the ring on screen stands.
#[derive(Clone, Copy)]
struct RingTimes {
    /// How often this session was snoozed so far.
    snoozed: u8,
    /// When its first ring started, across snoozes.
    first_fired: chrono::DateTime<chrono::Utc>,
    /// When the ring on screen started.
    ring_started: chrono::DateTime<chrono::Utc>,
}

impl RingTimes {
    fn load(alarm_id: &str) -> Self {
        let now = chrono::Utc::now();
        let session = crate::platform::alarm_kit_optional()
            .and_then(|kit| kit.session(&mobile_sentinel::InstanceId::new(alarm_id.to_owned())));
        let at = |ms: i64| chrono::DateTime::from_timestamp_millis(ms).unwrap_or(now);
        match session {
            Some(s) => Self {
                snoozed: u8::try_from(s.snooze_count).unwrap_or(u8::MAX),
                first_fired: at(s.first_fired_unix_ms),
                ring_started: at(s.fired_at_unix_ms),
            },
            None => Self {
                snoozed: 0,
                first_fired: now,
                ring_started: now,
            },
        }
    }

    /// What history keeps of an alarm the user dealt with just now. The total time counts
    /// from the first ring; the reaction time is to the last one, since sleeping through a
    /// snooze is no reaction.
    fn solved_event(
        self,
        alarm_id: &str,
        label: &str,
        first_interaction: Option<chrono::DateTime<chrono::Utc>>,
        snooze_interval_minutes: u8,
        challenge_solves: Vec<ChallengeSolve>,
    ) -> AlarmEvent {
        let now = chrono::Utc::now();
        AlarmEvent {
            id: uuid::Uuid::new_v4().to_string(),
            alarm_id: AlarmId(alarm_id.to_owned()),
            alarm_label: label.to_owned(),
            fire_timestamp: self.first_fired,
            first_interaction_timestamp: first_interaction
                .map(|at| self.first_fired + (at - self.ring_started)),
            dismiss_timestamp: now,
            snooze_count: self.snoozed,
            snooze_intervals_minutes: vec![snooze_interval_minutes; usize::from(self.snoozed)],
            challenge_solves,
            total_dismissal_time: (now - self.first_fired)
                .to_std()
                .unwrap_or(std::time::Duration::ZERO),
            outcome: AlarmOutcome::Solved,
        }
    }
}

/// The alarm that rings and what the screen needs to know about it.
pub struct RingState {
    id: String,
    label: String,
    hidden: bool,
    bright: bool,
    snooze_minutes: u8,
    snoozes_left: u8,
    setups: Vec<ChallengeSetup>,
    times: RingTimes,
    first_interaction: Option<chrono::DateTime<chrono::Utc>>,
}

/// "Thursday 5 Nov".
fn ring_date(tr: &crate::i18n::Translations, moment: chrono::DateTime<chrono::Local>) -> String {
    format!(
        "{} {} {}",
        t(
            tr,
            crate::text::FULL_DAY_KEYS[moment.weekday().num_days_from_monday() as usize]
        ),
        moment.day(),
        crate::text::month_short(tr, moment.month0() as usize),
    )
}

/// The alarm is over: what it wants afterwards (a wake-up check), and a one-time alarm
/// switches off or goes.
fn finish_alarm(ctx: &Ctx, id: &str) {
    let check_label = t(&ctx.tr.borrow(), "wake_check.label");
    let mut list = ctx.alarms.borrow_mut();
    let check = list
        .iter()
        .find(|a| a.id == id && a.wake_check_minutes > 0)
        .map(|a| {
            crate::alarm_ops::wake_check_alarm(a, chrono::Local::now().naive_local(), check_label)
        });
    let mut changed = crate::schedule::finish_if_one_time(&mut list, id);
    if let Some(check) = check {
        list.push(check);
        changed = true;
    }
    if changed {
        crate::persistence::save_alarms(&list);
    }
}

/// Fill the screen's texts from the state and the clock.
fn show_texts(ctx: &Ctx) {
    let Some(ui) = ctx.ui.upgrade() else {
        return;
    };
    let ring = ctx.ring.borrow();
    let Some(ring) = ring.as_ref() else {
        return;
    };
    let settings = ctx.settings.borrow();
    let tr = ctx.tr.borrow();
    let moment = chrono::Local::now();
    let twenty_four = crate::text::uses_24_hours(&settings);
    ui.set_ring_clock(
        crate::text::clock(moment.hour() as u8, moment.minute() as u8, twenty_four).into(),
    );
    ui.set_ring_date(ring_date(&tr, moment).into());
    ui.set_ring_snooze_text(
        t(&tr, "ring.snooze")
            .replace("{minutes}", &ring.snooze_minutes.to_string())
            .replace("{left}", &ring.snoozes_left.to_string())
            .into(),
    );
    ui.set_ring_can_snooze(ring.snoozes_left > 0);
    let challenges = ring.setups.len();
    ui.set_ring_challenge_count(challenges as i32);
    ui.set_ring_challenges_text(
        if challenges == 1 {
            t(&tr, "ring.one_challenge")
        } else {
            t(&tr, "ring.challenges").replace("{count}", &challenges.to_string())
        }
        .into(),
    );
}

fn start(ctx: &Rc<Ctx>, id: &str) {
    let Some(ui) = ctx.ui.upgrade() else {
        return;
    };
    // Another alarm took over the screen.
    super::challenge_run::abort(ctx);
    let alarm: Option<AlarmData> = ctx.alarms.borrow().iter().find(|a| a.id == id).cloned();
    let times = RingTimes::load(id);
    let max_snoozes = alarm.as_ref().map_or(0u8, |a| a.snooze_count);
    let label = alarm
        .as_ref()
        .map(|a| a.label.clone())
        .filter(|label| !label.is_empty())
        .unwrap_or_else(|| t(&ctx.tr.borrow(), "alarm.fallback_label"));
    let state = RingState {
        id: id.to_owned(),
        label: label.clone(),
        hidden: alarm.as_ref().is_some_and(|a| a.hidden),
        bright: alarm.as_ref().is_some_and(|a| a.max_brightness),
        snooze_minutes: alarm.as_ref().map_or(1u8, |a| a.snooze_interval),
        snoozes_left: max_snoozes.saturating_sub(times.snoozed),
        setups: alarm
            .as_ref()
            .map(|a| ChallengeSetup::list_from_stored(&a.challenges))
            .unwrap_or_default(),
        times,
        first_interaction: None,
    };
    // The screen came up: part of the record of how fast this ring was.
    if let Some(kit) = crate::platform::alarm_kit_optional() {
        kit.note_ui_up(&mobile_sentinel::InstanceId::new(id.to_owned()));
    }
    // Full brightness if this alarm asks for it, as long as this screen is up.
    if state.bright {
        mobile_sentinel::display::set_max_brightness();
    }
    ui.set_ring_label(label.into());
    ui.set_ring_note(alarm.map(|a| a.note).unwrap_or_default().into());
    *ctx.ring.borrow_mut() = Some(state);
    show_texts(ctx);
    ui.set_ringing(true);
}

fn stop(ctx: &Rc<Ctx>) {
    super::challenge_run::abort(ctx);
    let ring = ctx.ring.borrow_mut().take();
    if ring.is_some_and(|ring| ring.bright) {
        mobile_sentinel::display::restore_brightness();
    }
    if let Some(ui) = ctx.ui.upgrade() {
        ui.set_ringing(false);
    }
    super::home::refresh(ctx);
}

/// What to tell the user about alarms that ended without them, since the last time the app
/// was on screen.
pub fn unseen_outcome_notices(ctx: &Ctx) -> Vec<String> {
    let tr = ctx.tr.borrow();
    let unseen = crate::history_persistence::take_unseen_outcomes();
    let mut notices = Vec::new();
    for (outcome, key) in [
        (AlarmOutcome::Reboot, "alarm.ended_by_reboot"),
        (AlarmOutcome::Missed, "alarm.missed"),
        (AlarmOutcome::Limit, "alarm.ended_by_limit"),
    ] {
        let names: Vec<&str> = unseen
            .iter()
            .filter(|(o, _)| *o == outcome)
            .map(|(_, name)| name.as_str())
            .collect();
        if !names.is_empty() {
            notices.push(t(&tr, key).replace("{alarm}", &names.join(", ")));
        }
    }
    notices
}

/// One tick of the poll: what a ring end or a failed save asks the screen to say, and
/// whether an alarm started or stopped ringing.
pub fn poll(ctx: &Rc<Ctx>) {
    if crate::persistence::take_save_failure() {
        let message = t(&ctx.tr.borrow(), "db.save_failed");
        super::toast::error(ctx, message);
    }
    if crate::platform::take_ring_end_news() {
        *ctx.alarms.borrow_mut() = crate::persistence::load_alarms();
        super::home::refresh(ctx);
        // Rang until its limit with nobody there: nothing else tells the user.
        let notices = unseen_outcome_notices(ctx);
        if !notices.is_empty() {
            super::toast::error(ctx, notices.join(" "));
        }
    }
    let tick = {
        let mut watch = ctx.firing_watch.borrow_mut();
        // One `stat` per tick; the records are only re-read on change.
        watch.should_reread(crate::firing_state::store_marker())
    };
    if tick {
        let current = crate::firing_state::peek_firing_alarm();
        let showing = ctx.ring.borrow().as_ref().map(|ring| ring.id.clone());
        if current != showing {
            match current {
                Some(id) => start(ctx, &id),
                None => stop(ctx),
            }
        }
    }
    if ctx.ring.borrow().is_some() {
        // Proof of life while an alarm rings. If this UI freezes, the beat stops and the
        // `:sentinel` guardian restarts the app (the alarm keeps ringing and the lockdown
        // stays; there is no way out but solving it). This runs on the UI thread itself.
        #[cfg(debug_assertions)]
        crate::debug_hooks::block_if_requested();
        mobile_sentinel::jobs::ui_heartbeat();
        let ticks = ctx.ticks.get().wrapping_add(1);
        ctx.ticks.set(ticks);
        // A live clock, so a glance tells how long this has been going; the brightness again,
        // as the window may only be on top by now.
        if ticks % 10 == 0 {
            show_texts(ctx);
            if ctx.ring.borrow().as_ref().is_some_and(|ring| ring.bright) {
                mobile_sentinel::display::set_max_brightness();
            }
        }
    }
}

pub fn snooze(ctx: &Rc<Ctx>) {
    let id = {
        let mut ring = ctx.ring.borrow_mut();
        let Some(ring) = ring.as_mut() else {
            return;
        };
        if ring.snoozes_left == 0 {
            return;
        }
        ring.snoozes_left -= 1;
        ring.id.clone()
    };
    show_texts(ctx);
    let kit = crate::platform::alarm_kit();
    let instance = mobile_sentinel::InstanceId::new(id.clone());
    if let Err(e) = kit.snooze(&instance) {
        log::warn!("[AlarmFree] AlarmKit snooze failed for {id}: {e:?}");
    }
    // With another alarm still ringing, the screen moves on to that one.
    if matches!(kit.current_session(), Ok(None)) {
        mobile_sentinel::foregrounding::finish_activity();
    }
}

pub fn turn_off(ctx: &Rc<Ctx>) {
    let (id, setups) = {
        let mut ring = ctx.ring.borrow_mut();
        let Some(ring) = ring.as_mut() else {
            return;
        };
        if ring.first_interaction.is_none() {
            ring.first_interaction = Some(chrono::Utc::now());
        }
        (ring.id.clone(), ring.setups.clone())
    };
    let locale = ctx.settings.borrow().locale;
    let prepared = crate::challenges::prepare(&setups, locale);
    if prepared.is_empty() {
        // No challenge, or none this version knows: the SDK refuses to dismiss a challenge
        // alarm until it is marked solved, so mark it first or the user would be stuck in
        // the lockdown.
        solved(ctx, &id, Vec::new());
    } else {
        // The sound pauses while the challenges are done.
        let _ = crate::platform::alarm_kit().pause(&mobile_sentinel::InstanceId::new(id.clone()));
        super::challenge_run::start_ring(ctx, &id, prepared);
    }
}

/// The user did what it takes: the alarm is over.
pub fn solved(ctx: &Rc<Ctx>, id: &str, solves: Vec<ChallengeSolve>) {
    let kit = crate::platform::alarm_kit();
    let instance = mobile_sentinel::InstanceId::new(id.to_owned());
    let _ = kit.mark_solved(&instance);
    let _ = kit.dismiss(&instance);
    finished(ctx, id, solves);
}

/// Everything that happens when the alarm is over, whichever way that went.
fn finished(ctx: &Rc<Ctx>, id: &str, solves: Vec<ChallengeSolve>) {
    if let Some(ring) = ctx.ring.borrow().as_ref() {
        if !ring.hidden {
            crate::history_persistence::record_alarm_event(&ring.times.solved_event(
                id,
                &ring.label,
                ring.first_interaction,
                ring.snooze_minutes,
                solves,
            ));
        }
    }
    finish_alarm(ctx, id);
    stop(ctx);
}
