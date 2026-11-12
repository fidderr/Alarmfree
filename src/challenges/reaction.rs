//! The reaction challenge: pop a number of targets within the time limit, for a number
//! of rounds. Miss the time and it starts over at round 1.

/// Targets on screen at once. Popping one spawns a replacement until the round's quota is
/// met, so the player never waits.
pub const VISIBLE_TARGETS: usize = 3;

/// Lead-in before the very first round.
pub const FIRST_COUNTDOWN_MS: u64 = 3000;
/// Shorter lead-in between the rounds.
pub const ROUND_LEAD_MS: u64 = 1500;
/// How often the game is stepped.
pub const POLL_MS: u64 = 40;

/// Smallest distance between the centres of targets, as a fraction of the play area.
const MIN_SEPARATION: f32 = 0.34;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Target {
    pub id: u32,
    pub fx: f32,
    pub fy: f32,
}

/// A random position that stays at least `MIN_SEPARATION` from every target. After 32 tries
/// the best one found: as far from the nearest as it got.
pub fn pick_position(existing: &[Target]) -> (f32, f32) {
    let mut best = (rand::random::<f32>(), rand::random::<f32>());
    let mut best_min_dist = f32::NEG_INFINITY;
    for _ in 0..32 {
        let cand = (rand::random::<f32>(), rand::random::<f32>());
        let min_dist = existing
            .iter()
            .map(|t| {
                let dx = t.fx - cand.0;
                let dy = t.fy - cand.1;
                (dx * dx + dy * dy).sqrt()
            })
            .fold(f32::INFINITY, f32::min);
        if existing.is_empty() || min_dist >= MIN_SEPARATION {
            return cand;
        }
        if min_dist > best_min_dist {
            best_min_dist = min_dist;
            best = cand;
        }
    }
    best
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Phase {
    Countdown { remaining_ms: u64 },
    Playing { elapsed_ms: u64 },
    Done,
}

/// What a step of the clock led to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    None,
    /// The countdown ended: the targets are there and the time runs.
    Started,
    /// The time ran out: back to round 1.
    TimedOut,
    RoundCleared,
    Solved,
}

#[derive(Debug, Clone)]
pub struct ReactionGame {
    per_round: u32,
    pub rounds: u32,
    pub time_limit_ms: u64,
    slot_count: usize,
    pub phase: Phase,
    pub round: u32,
    pub popped: u32,
    pub slots: Vec<Option<Target>>,
    next_id: u32,
}

impl ReactionGame {
    pub fn new(targets_per_round: u32, time_limit_ms: u32, rounds: u32) -> Self {
        let per_round = targets_per_round.max(1);
        let slot_count = VISIBLE_TARGETS.min(per_round as usize);
        Self {
            per_round,
            rounds: rounds.max(1),
            time_limit_ms: u64::from(time_limit_ms.max(1000)),
            slot_count,
            phase: Phase::Countdown {
                remaining_ms: FIRST_COUNTDOWN_MS,
            },
            round: 1,
            popped: 0,
            slots: vec![None; slot_count],
            next_id: 0,
        }
    }

    pub fn per_round(&self) -> u32 {
        self.per_round
    }

    pub fn counting_down(&self) -> bool {
        matches!(self.phase, Phase::Countdown { .. })
    }

    /// The number shown in the "get ready" countdown.
    pub fn countdown_secs(&self) -> u32 {
        match self.phase {
            Phase::Countdown { remaining_ms } => remaining_ms.div_ceil(1000) as u32,
            _ => 0,
        }
    }

    /// How much of the round's time is left, 0 to 1.
    pub fn time_left(&self) -> f32 {
        match self.phase {
            Phase::Playing { elapsed_ms } => {
                1.0 - (elapsed_ms as f32 / self.time_limit_ms as f32).min(1.0)
            }
            _ => 1.0,
        }
    }

    fn occupied(&self) -> Vec<Target> {
        self.slots.iter().flatten().copied().collect()
    }

    fn spawn(&mut self, slot: usize) {
        self.slots[slot] = None;
        let (fx, fy) = pick_position(&self.occupied());
        let id = self.next_id;
        self.next_id += 1;
        self.slots[slot] = Some(Target { id, fx, fy });
    }

    fn clear(&mut self) {
        self.slots.fill(None);
    }

    /// Let `ms` of the clock go by.
    pub fn step(&mut self, ms: u64) -> Event {
        match self.phase {
            Phase::Done => Event::None,
            Phase::Countdown { remaining_ms } => {
                if remaining_ms > ms {
                    self.phase = Phase::Countdown {
                        remaining_ms: remaining_ms - ms,
                    };
                    return Event::None;
                }
                for i in 0..self.slot_count {
                    self.spawn(i);
                }
                self.popped = 0;
                self.phase = Phase::Playing { elapsed_ms: 0 };
                Event::Started
            }
            Phase::Playing { elapsed_ms } => {
                if self.popped >= self.per_round {
                    self.clear();
                    if self.round >= self.rounds {
                        self.phase = Phase::Done;
                        return Event::Solved;
                    }
                    self.round += 1;
                    self.popped = 0;
                    self.phase = Phase::Countdown {
                        remaining_ms: ROUND_LEAD_MS,
                    };
                    return Event::RoundCleared;
                }
                if elapsed_ms >= self.time_limit_ms {
                    self.clear();
                    self.round = 1;
                    self.popped = 0;
                    self.phase = Phase::Countdown {
                        remaining_ms: FIRST_COUNTDOWN_MS,
                    };
                    return Event::TimedOut;
                }
                self.phase = Phase::Playing {
                    elapsed_ms: elapsed_ms + ms,
                };
                Event::None
            }
        }
    }

    /// A tap on target `id`. True when it was on the board.
    pub fn hit(&mut self, id: u32) -> bool {
        if !matches!(self.phase, Phase::Playing { .. }) {
            return false;
        }
        let Some(slot) = self.slots.iter().position(|s| s.map(|t| t.id) == Some(id)) else {
            return false;
        };
        self.popped += 1;
        // How many must still show after this one: the board size, or what is left to pop.
        let remaining = self.per_round.saturating_sub(self.popped) as usize;
        let desired = self.slot_count.min(remaining);
        let visible_others = self.slots.iter().filter(|s| s.is_some()).count() - 1;
        if visible_others < desired {
            self.spawn(slot);
        } else {
            self.slots[slot] = None;
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Run the clock until the targets are out.
    fn started(game: &mut ReactionGame) {
        loop {
            if game.step(POLL_MS) == Event::Started {
                return;
            }
        }
    }

    fn visible(game: &ReactionGame) -> Vec<u32> {
        game.slots.iter().flatten().map(|t| t.id).collect()
    }

    #[test]
    fn it_opens_with_a_three_second_countdown() {
        let mut game = ReactionGame::new(5, 3000, 2);
        assert!(game.counting_down());
        assert_eq!(game.countdown_secs(), 3);
        game.step(1000);
        assert_eq!(game.countdown_secs(), 2);
        game.step(1000);
        assert_eq!(game.countdown_secs(), 1);
        assert_eq!(game.step(1000), Event::Started);
        assert!(!game.counting_down());
    }

    #[test]
    fn up_to_three_targets_show_and_each_pop_spawns_a_replacement() {
        let mut game = ReactionGame::new(5, 3000, 1);
        started(&mut game);
        assert_eq!(visible(&game).len(), 3);
        let first = visible(&game)[0];
        assert!(game.hit(first));
        // 4 left to pop, 3 can show: the slot is refilled with a new target.
        assert_eq!(visible(&game).len(), 3);
        assert!(!visible(&game).contains(&first));
        assert_eq!(game.popped, 1);
    }

    #[test]
    fn near_the_end_fewer_targets_show() {
        let mut game = ReactionGame::new(4, 3000, 1);
        started(&mut game);
        for _ in 0..2 {
            let id = visible(&game)[0];
            game.hit(id);
        }
        // 2 popped, 2 to go: 2 on the board.
        assert_eq!(visible(&game).len(), 2);
        let id = visible(&game)[0];
        game.hit(id);
        assert_eq!(visible(&game).len(), 1);
    }

    #[test]
    fn few_targets_per_round_means_a_smaller_board() {
        let game = ReactionGame::new(2, 3000, 1);
        assert_eq!(game.slots.len(), 2);
    }

    #[test]
    fn clearing_the_only_round_solves_it() {
        let mut game = ReactionGame::new(3, 3000, 1);
        started(&mut game);
        while let Some(id) = visible(&game).first().copied() {
            assert!(game.hit(id));
        }
        assert_eq!(game.popped, 3);
        assert_eq!(game.step(POLL_MS), Event::Solved);
        assert_eq!(game.step(POLL_MS), Event::None);
    }

    #[test]
    fn a_cleared_round_leads_to_a_short_countdown_and_the_next_round() {
        let mut game = ReactionGame::new(3, 3000, 2);
        started(&mut game);
        while let Some(id) = visible(&game).first().copied() {
            game.hit(id);
        }
        assert_eq!(game.step(POLL_MS), Event::RoundCleared);
        assert_eq!(game.round, 2);
        assert_eq!(game.phase, Phase::Countdown { remaining_ms: 1500 });
        assert_eq!(game.countdown_secs(), 2);
    }

    #[test]
    fn running_out_of_time_starts_over_at_round_one() {
        let mut game = ReactionGame::new(5, 2000, 3);
        started(&mut game);
        game.round = 2;
        let mut event = Event::None;
        for _ in 0..200 {
            event = game.step(POLL_MS);
            if event != Event::None {
                break;
            }
        }
        assert_eq!(event, Event::TimedOut);
        assert_eq!(game.round, 1);
        assert_eq!(game.popped, 0);
        assert_eq!(game.phase, Phase::Countdown { remaining_ms: 3000 });
        assert!(visible(&game).is_empty());
    }

    #[test]
    fn taps_do_nothing_during_the_countdown_or_on_a_gone_target() {
        let mut game = ReactionGame::new(5, 3000, 1);
        assert!(!game.hit(0));
        started(&mut game);
        assert!(!game.hit(99));
        assert_eq!(game.popped, 0);
    }

    #[test]
    fn the_time_bar_drains_over_the_limit() {
        let mut game = ReactionGame::new(5, 2000, 1);
        assert_eq!(game.time_left(), 1.0);
        started(&mut game);
        assert_eq!(game.time_left(), 1.0);
        for _ in 0..25 {
            game.step(POLL_MS);
        }
        assert!((game.time_left() - 0.5).abs() < 0.03);
    }

    #[test]
    fn fresh_targets_keep_their_distance() {
        let mut existing = Vec::new();
        for id in 0..3 {
            let (fx, fy) = pick_position(&existing);
            existing.push(Target { id, fx, fy });
        }
        let (a, b, c) = (existing[0], existing[1], existing[2]);
        let dist = |p: Target, q: Target| ((p.fx - q.fx).powi(2) + (p.fy - q.fy).powi(2)).sqrt();
        // The best of 32 tries is almost always far enough; never stacked.
        assert!(dist(a, b) > 0.05 && dist(b, c) > 0.05 && dist(a, c) > 0.05);
    }

    #[test]
    fn a_short_limit_is_made_a_second() {
        assert_eq!(ReactionGame::new(5, 100, 1).time_limit_ms, 1000);
    }
}
