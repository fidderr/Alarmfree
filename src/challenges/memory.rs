//! The memory game: a sequence lights up cell by cell, then the user taps it back.

/// Wrong taps in a row before the sequence is shown again.
pub const MAX_WRONG: usize = 3;

/// Before the first cell lights.
pub const START_MS: u64 = 300;
/// Each cell stays lit this long.
pub const CELL_MS: u64 = 1000;
/// After the last cell, before input opens.
pub const END_MS: u64 = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Display,
    Input,
}

/// What a tap did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tap {
    /// Not in the input phase, or a cell that is already marked.
    Ignored,
    Correct,
    /// The last cell of the sequence.
    Complete,
    Wrong,
    /// Too many wrong ones: the sequence plays again from the start.
    Replay,
}

#[derive(Debug, Clone)]
pub struct MemoryState {
    sequence: Vec<u8>,
    pub phase: Phase,
    /// The cell lit now; past the end means none.
    pub display_index: usize,
    pub user: Vec<u8>,
    pub wrong: Vec<u8>,
}

impl MemoryState {
    pub fn new(sequence: Vec<u8>) -> Self {
        Self {
            sequence,
            phase: Phase::Display,
            display_index: 0,
            user: Vec::new(),
            wrong: Vec::new(),
        }
    }

    pub fn sequence_len(&self) -> usize {
        self.sequence.len()
    }

    /// Back to the start of the display phase.
    pub fn restart(&mut self) {
        self.phase = Phase::Display;
        self.display_index = 0;
        self.user.clear();
        self.wrong.clear();
    }

    /// The cell lit right now, if any.
    pub fn lit(&self) -> Option<u8> {
        if self.phase == Phase::Display {
            self.sequence.get(self.display_index).copied()
        } else {
            None
        }
    }

    /// The display phase as steps: how long to wait before each, and what it does. The
    /// first lights cell 0, the next ones the following cells, one dark step follows the
    /// last cell, and the final one opens the input.
    pub fn display_steps(&self) -> Vec<(u64, DisplayStep)> {
        let len = self.sequence.len();
        let mut steps = vec![(START_MS, DisplayStep::Light(0))];
        for i in 1..len {
            steps.push((CELL_MS, DisplayStep::Light(i)));
        }
        steps.push((CELL_MS, DisplayStep::Dark));
        steps.push((END_MS, DisplayStep::Open));
        steps
    }

    pub fn apply(&mut self, step: DisplayStep) {
        match step {
            DisplayStep::Light(i) => self.display_index = i,
            DisplayStep::Dark => self.display_index = self.sequence.len(),
            DisplayStep::Open => self.phase = Phase::Input,
        }
    }

    pub fn tap(&mut self, cell: u8) -> Tap {
        if self.phase != Phase::Input || self.wrong.contains(&cell) || self.user.contains(&cell) {
            return Tap::Ignored;
        }
        if self.sequence.get(self.user.len()) == Some(&cell) {
            self.user.push(cell);
            // A correct tap clears the wrong marks, so a cell tapped too early and red is not
            // stuck once it becomes the right one.
            self.wrong.clear();
            if self.user.len() == self.sequence.len() {
                Tap::Complete
            } else {
                Tap::Correct
            }
        } else {
            self.wrong.push(cell);
            if self.wrong.len() >= MAX_WRONG {
                self.restart();
                Tap::Replay
            } else {
                Tap::Wrong
            }
        }
    }

    /// 0 idle, 1 lit, 2 tapped right, 3 tapped wrong.
    pub fn cell_state(&self, cell: u8) -> i32 {
        if self.wrong.contains(&cell) {
            3
        } else if self.lit() == Some(cell) {
            1
        } else if self.phase == Phase::Input && self.user.contains(&cell) {
            2
        } else {
            0
        }
    }

    /// The order number shown on a right cell.
    pub fn order_of(&self, cell: u8) -> Option<usize> {
        if self.phase != Phase::Input {
            return None;
        }
        self.user.iter().position(|c| *c == cell).map(|p| p + 1)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisplayStep {
    Light(usize),
    Dark,
    Open,
}

/// Columns and cells of the grid.
pub fn grid(grid_size: u8) -> (usize, u8) {
    if grid_size >= 4 {
        (4, 12)
    } else {
        (3, 9)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ready(sequence: &[u8]) -> MemoryState {
        let mut state = MemoryState::new(sequence.to_vec());
        for (_, step) in state.display_steps() {
            state.apply(step);
        }
        assert_eq!(state.phase, Phase::Input);
        state
    }

    #[test]
    fn the_display_lights_each_cell_then_opens() {
        let mut state = MemoryState::new(vec![4, 2, 7]);
        let steps = state.display_steps();
        assert_eq!(steps.len(), 5);
        assert_eq!(steps[0], (300, DisplayStep::Light(0)));
        assert_eq!(steps[1], (1000, DisplayStep::Light(1)));
        assert_eq!(steps[3], (1000, DisplayStep::Dark));
        assert_eq!(steps[4], (500, DisplayStep::Open));
        state.apply(steps[0].1);
        assert_eq!(state.lit(), Some(4));
        state.apply(steps[2].1);
        assert_eq!(state.lit(), Some(7));
        state.apply(steps[3].1);
        assert_eq!(state.lit(), None);
        assert_eq!(state.phase, Phase::Display);
        state.apply(steps[4].1);
        assert_eq!(state.phase, Phase::Input);
    }

    #[test]
    fn taps_before_the_input_phase_do_nothing() {
        let mut state = MemoryState::new(vec![1, 2]);
        assert_eq!(state.tap(1), Tap::Ignored);
        assert!(state.user.is_empty());
    }

    #[test]
    fn the_whole_sequence_in_order_completes() {
        let mut state = ready(&[3, 0, 8]);
        assert_eq!(state.tap(3), Tap::Correct);
        assert_eq!(state.order_of(3), Some(1));
        // A tapped cell cannot be tapped again.
        assert_eq!(state.tap(3), Tap::Ignored);
        assert_eq!(state.tap(0), Tap::Correct);
        assert_eq!(state.tap(8), Tap::Complete);
    }

    #[test]
    fn a_wrong_cell_goes_red_and_is_locked() {
        let mut state = ready(&[3, 0, 8]);
        assert_eq!(state.tap(5), Tap::Wrong);
        assert_eq!(state.cell_state(5), 3);
        assert_eq!(state.tap(5), Tap::Ignored);
        assert_eq!(state.wrong.len(), 1);
    }

    #[test]
    fn a_right_tap_clears_the_wrong_marks_and_the_count() {
        let mut state = ready(&[3, 0, 8]);
        state.tap(5);
        state.tap(6);
        assert_eq!(state.wrong.len(), 2);
        assert_eq!(state.tap(3), Tap::Correct);
        assert!(state.wrong.is_empty());
        // Two more wrongs do not replay now: the budget is full again.
        assert_eq!(state.tap(1), Tap::Wrong);
        assert_eq!(state.tap(2), Tap::Wrong);
    }

    #[test]
    fn three_wrong_taps_replay_from_the_start() {
        let mut state = ready(&[3, 0, 8]);
        state.tap(3);
        assert_eq!(state.tap(1), Tap::Wrong);
        assert_eq!(state.tap(2), Tap::Wrong);
        assert_eq!(state.tap(4), Tap::Replay);
        assert_eq!(state.phase, Phase::Display);
        assert!(state.user.is_empty() && state.wrong.is_empty());
        assert_eq!(state.display_index, 0);
    }

    #[test]
    fn a_cell_tapped_early_can_be_right_later() {
        let mut state = ready(&[3, 0, 8]);
        // 0 is wrong now, but the second in the sequence.
        assert_eq!(state.tap(0), Tap::Wrong);
        assert_eq!(state.tap(3), Tap::Correct);
        assert_eq!(state.tap(0), Tap::Correct);
    }

    #[test]
    fn the_grid_has_nine_or_twelve_cells() {
        assert_eq!(grid(3), (3, 9));
        assert_eq!(grid(4), (4, 12));
    }
}
