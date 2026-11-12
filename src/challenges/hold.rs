//! The hold-the-bubble challenge: press a bubble and keep holding until its ring fills.

/// The pop lasts this long.
pub const POP_MS: u64 = 280;

/// A hold shorter than this is not asked for, however it was set.
pub const MIN_HOLD_MS: u32 = 300;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bubble {
    Idle,
    Filling,
    Popping,
}

/// What a press did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Press {
    Ignored,
    /// Started filling; the hold token to finish with after the hold time.
    Started(u32),
}

/// What a finished hold led to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Finish {
    /// Released early, or a newer hold took over.
    Stale,
    /// Popped; a fresh bubble follows.
    Popped,
    /// Popped the last one.
    Last,
}

#[derive(Debug, Clone)]
pub struct HoldState {
    pub required: u32,
    pub hold_ms: u32,
    pub completed: u32,
    pub bubble: Bubble,
    /// The centre as fractions across the play area's safe band.
    pub pos: (f32, f32),
    token: u32,
}

impl HoldState {
    pub fn new(required: u32, hold_ms: u32) -> Self {
        Self {
            required: required.max(1),
            hold_ms: hold_ms.max(MIN_HOLD_MS),
            completed: 0,
            bubble: Bubble::Idle,
            pos: random_pos(),
            token: 0,
        }
    }

    pub fn press(&mut self) -> Press {
        if self.bubble != Bubble::Idle {
            return Press::Ignored;
        }
        self.token += 1;
        self.bubble = Bubble::Filling;
        Press::Started(self.token)
    }

    /// Letting go drains the ring and cancels the hold in flight.
    pub fn release(&mut self) {
        if self.bubble == Bubble::Filling {
            self.token += 1;
            self.bubble = Bubble::Idle;
        }
    }

    /// The hold time of hold `token` has passed.
    pub fn finish(&mut self, token: u32) -> Finish {
        if self.token != token || self.bubble != Bubble::Filling {
            return Finish::Stale;
        }
        self.completed += 1;
        self.bubble = Bubble::Popping;
        if self.completed >= self.required {
            Finish::Last
        } else {
            Finish::Popped
        }
    }

    /// The pop is over: a fresh bubble elsewhere.
    pub fn next_bubble(&mut self) {
        self.bubble = Bubble::Idle;
        self.pos = random_pos();
    }
}

/// Random position as 0..1 fractions across the safe band on each axis.
pub fn random_pos() -> (f32, f32) {
    (rand::random::<f32>(), rand::random::<f32>())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn too_short_a_hold_is_made_longer() {
        assert_eq!(HoldState::new(3, 10).hold_ms, MIN_HOLD_MS);
        assert_eq!(HoldState::new(0, 3000).required, 1);
    }

    #[test]
    fn a_hold_that_lasts_pops_the_bubble() {
        let mut state = HoldState::new(2, 1000);
        let Press::Started(token) = state.press() else {
            panic!("press ignored");
        };
        assert_eq!(state.bubble, Bubble::Filling);
        assert_eq!(state.finish(token), Finish::Popped);
        assert_eq!(state.completed, 1);
        assert_eq!(state.bubble, Bubble::Popping);
        state.next_bubble();
        assert_eq!(state.bubble, Bubble::Idle);
    }

    #[test]
    fn letting_go_early_cancels_the_hold() {
        let mut state = HoldState::new(2, 1000);
        let Press::Started(token) = state.press() else {
            panic!("press ignored");
        };
        state.release();
        assert_eq!(state.bubble, Bubble::Idle);
        assert_eq!(state.finish(token), Finish::Stale);
        assert_eq!(state.completed, 0);
    }

    #[test]
    fn an_old_timer_does_not_finish_a_new_hold() {
        let mut state = HoldState::new(2, 1000);
        let Press::Started(old) = state.press() else {
            panic!("press ignored");
        };
        state.release();
        let Press::Started(new) = state.press() else {
            panic!("press ignored");
        };
        assert_eq!(state.finish(old), Finish::Stale);
        assert_eq!(state.finish(new), Finish::Popped);
    }

    #[test]
    fn pressing_again_while_filling_or_popping_is_ignored() {
        let mut state = HoldState::new(2, 1000);
        let Press::Started(token) = state.press() else {
            panic!("press ignored");
        };
        assert_eq!(state.press(), Press::Ignored);
        state.finish(token);
        assert_eq!(state.press(), Press::Ignored);
    }

    #[test]
    fn the_last_bubble_ends_it() {
        let mut state = HoldState::new(1, 1000);
        let Press::Started(token) = state.press() else {
            panic!("press ignored");
        };
        assert_eq!(state.finish(token), Finish::Last);
    }

    #[test]
    fn positions_stay_inside_the_band() {
        for _ in 0..50 {
            let (y, x) = random_pos();
            assert!((0.0..=1.0).contains(&x) && (0.0..=1.0).contains(&y));
        }
    }
}
