//! The sum and its keypad. Done as soon as the typed digits are the answer; the check key
//! only tells that what is typed is not it.

/// How long an answer can get.
pub const MAX_DIGITS: usize = 12;

#[derive(Debug, Clone, PartialEq)]
pub struct MathState {
    expected: i64,
    pub typed: String,
    pub wrong: bool,
    pub solved: bool,
}

impl MathState {
    pub fn new(expected: i64) -> Self {
        Self {
            expected,
            typed: String::new(),
            wrong: false,
            solved: false,
        }
    }

    /// A digit key. Returns true when this key made the answer.
    pub fn press(&mut self, digit: char) -> bool {
        if self.solved {
            return false;
        }
        self.wrong = false;
        if self.typed.chars().count() < MAX_DIGITS {
            self.typed.push(digit);
        }
        if self.typed.parse::<i64>() == Ok(self.expected) {
            self.solved = true;
        }
        self.solved
    }

    pub fn delete(&mut self) {
        self.typed.pop();
        self.wrong = false;
    }

    /// The check key: what is typed is not the answer (or it would be done already).
    pub fn check(&mut self) {
        if !self.solved {
            self.wrong = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn type_all(state: &mut MathState, digits: &str) -> bool {
        digits.chars().fold(false, |_, d| state.press(d))
    }

    #[test]
    fn the_answer_solves_it_at_once() {
        let mut state = MathState::new(91);
        assert!(!state.press('9'));
        assert!(state.press('1'));
        assert!(state.solved);
        // Nothing counts after that.
        assert!(!state.press('2'));
        assert_eq!(state.typed, "91");
    }

    #[test]
    fn a_wrong_answer_is_only_called_wrong_on_check() {
        let mut state = MathState::new(91);
        type_all(&mut state, "92");
        assert!(!state.wrong);
        state.check();
        assert!(state.wrong);
        // Typing or deleting takes the mark away.
        state.delete();
        assert!(!state.wrong);
        assert_eq!(state.typed, "9");
    }

    #[test]
    fn an_answer_is_at_most_twelve_digits() {
        let mut state = MathState::new(5);
        type_all(&mut state, "11111111111111");
        assert_eq!(state.typed.len(), MAX_DIGITS);
        assert!(!state.solved);
    }

    #[test]
    fn deleting_an_empty_answer_is_fine() {
        let mut state = MathState::new(5);
        state.delete();
        assert_eq!(state.typed, "");
    }

    #[test]
    fn leading_zeros_still_count_as_the_number() {
        let mut state = MathState::new(7);
        assert!(!state.press('0'));
        assert!(state.press('7'));
    }
}
