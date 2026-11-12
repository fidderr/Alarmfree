//! The typing challenge: how the passage is laid out and coloured, and when it is done.

use crate::models::typing_accuracy;

/// One character of the passage on the grid it is drawn on.
#[derive(Debug, Clone, PartialEq)]
pub struct Cell {
    pub ch: char,
    pub col: usize,
    pub row: usize,
    /// 0 still to type, 1 right, 2 wrong, 3 next to type.
    pub state: i32,
}

/// Lay the passage out in rows of at most `cols` characters. A word never breaks across
/// rows; the space after a word stays at the end of its row.
pub fn layout(passage: &str, cols: usize) -> Vec<(char, usize, usize)> {
    let cols = cols.max(1);
    let chars: Vec<char> = passage.chars().collect();
    let mut cells = Vec::with_capacity(chars.len());
    let (mut row, mut col) = (0usize, 0usize);
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == ' ' {
            // A space at the start of a row is dropped from the layout width, not from the text,
            // so it hangs off the end of the previous one.
            cells.push((' ', row, col.min(cols)));
            col += 1;
            i += 1;
            continue;
        }
        let end = chars[i..]
            .iter()
            .position(|c| *c == ' ')
            .map_or(chars.len(), |p| i + p);
        let word_len = end - i;
        if col > 0 && col + word_len > cols {
            row += 1;
            col = 0;
        }
        for ch in &chars[i..end] {
            cells.push((*ch, row, col));
            col += 1;
        }
        i = end;
    }
    cells
}

/// The state of every character of the passage for what is typed so far. Capitals do not
/// count, as with the accuracy.
pub fn colour(passage: &str, typed: &str, cols: usize) -> Vec<Cell> {
    let typed: Vec<char> = typed.chars().collect();
    let passage_chars: Vec<char> = passage.chars().collect();
    layout(passage, cols)
        .into_iter()
        .enumerate()
        .map(|(i, (ch, row, col))| {
            let state = if i < typed.len() {
                let same = typed[i].eq_ignore_ascii_case(&passage_chars[i]);
                if same {
                    1
                } else {
                    2
                }
            } else if i == typed.len() {
                3
            } else {
                0
            };
            Cell {
                ch,
                col,
                row,
                state,
            }
        })
        .collect()
}

/// Where the typing stands.
#[derive(Debug, Clone, PartialEq)]
pub struct Progress {
    pub typed: usize,
    pub total: usize,
    /// Accuracy so far, in percent, against as much of the passage as is typed.
    pub accuracy_pct: u32,
    pub good: bool,
    pub min_pct: u32,
    /// All of it typed, with too much of it wrong.
    pub wrong: bool,
    pub done: bool,
}

pub fn progress(passage: &str, typed: &str, min_accuracy: f32) -> Progress {
    let typed_len = typed.chars().count();
    let total = passage.chars().count();
    let so_far: String = passage.chars().take(typed_len).collect();
    let accuracy = typing_accuracy(&so_far, typed);
    // A skipped letter must not leave the user stuck one letter short of the end.
    let to_spare = (total as f32 * (1.0 - min_accuracy)) as usize;
    let accurate = typing_accuracy(passage, typed) >= min_accuracy;
    Progress {
        typed: typed_len,
        total,
        accuracy_pct: (accuracy * 100.0) as u32,
        good: accuracy >= min_accuracy,
        min_pct: (min_accuracy * 100.0) as u32,
        wrong: typed_len >= total && !accurate,
        done: accurate && typed_len + to_spare >= total,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn words_wrap_whole() {
        let cells = layout("aaa bbb ccc", 7);
        let row_of = |i: usize| cells[i].1;
        // "aaa bbb" fits, "ccc" goes to the next row.
        assert_eq!(row_of(0), 0);
        assert_eq!(row_of(6), 0);
        assert_eq!(row_of(8), 1);
        assert_eq!(cells[8].2, 0);
        assert_eq!(cells.len(), 11);
    }

    #[test]
    fn a_word_longer_than_the_row_still_stays_whole() {
        let cells = layout("ab abcdefghij", 5);
        assert_eq!(cells[3].1, 1);
        assert!(cells[3..].iter().all(|c| c.1 == 1));
    }

    #[test]
    fn colours_follow_what_is_typed() {
        let cells = colour("Hi you", "hx", 20);
        let states: Vec<i32> = cells.iter().map(|c| c.state).collect();
        assert_eq!(states, [1, 2, 3, 0, 0, 0]);
    }

    #[test]
    fn a_space_typed_for_a_letter_is_wrong() {
        let cells = colour("ab", "a ", 20);
        assert_eq!(cells[1].state, 2);
    }

    #[test]
    fn done_when_accurate_and_nearly_at_the_end() {
        let passage = "good morning to you all";
        let p = progress(passage, passage, 0.85);
        assert!(p.done && !p.wrong);
        assert_eq!(p.accuracy_pct, 100);
        // Half way: accurate but not done.
        let half = progress(passage, "good morning", 0.85);
        assert!(!half.done && !half.wrong);
        assert!(half.good);
    }

    #[test]
    fn a_few_letters_short_still_finishes() {
        let passage = "abcdefghijklmnopqrst"; // 20 letters, 80 percent: 4 to spare
        let p = progress(passage, "abcdefghijklmnopq", 0.8);
        assert!(p.done);
    }

    #[test]
    fn all_typed_but_too_wrong_is_incorrect() {
        let p = progress("hello world", "xxxxxxxxxxx", 0.9);
        assert!(p.wrong && !p.done && !p.good);
    }

    #[test]
    fn capitals_do_not_count_against() {
        let p = progress("hello world", "HELLO WORLD", 0.95);
        assert!(p.done);
    }
}
