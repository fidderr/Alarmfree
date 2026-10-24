//! Reordering items of an editable list.

/// Move the item at `from` so that it ends up at position `to` (the others
/// shift to make room). Returns whether anything moved (false for equal
/// positions or an index out of range).
pub fn move_item<T>(list: &mut Vec<T>, from: usize, to: usize) -> bool {
    if from == to || from >= list.len() || to >= list.len() {
        return false;
    }
    let item = list.remove(from);
    list.insert(to, item);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_item_moves_to_the_requested_position() {
        let mut v = vec!["a", "b", "c", "d"];
        assert!(move_item(&mut v, 0, 2));
        assert_eq!(v, ["b", "c", "a", "d"]);
        assert!(move_item(&mut v, 3, 0));
        assert_eq!(v, ["d", "b", "c", "a"]);
        assert!(move_item(&mut v, 1, 0));
        assert_eq!(v, ["b", "d", "c", "a"]);
    }

    #[test]
    fn the_same_position_and_bad_indexes_do_not_move() {
        let mut v = vec![1, 2, 3];
        assert!(!move_item(&mut v, 1, 1));
        assert!(!move_item(&mut v, 0, 9));
        assert!(!move_item(&mut v, 9, 0));
        assert_eq!(v, [1, 2, 3]);
        let mut empty: Vec<u8> = vec![];
        assert!(!move_item(&mut empty, 0, 0));
    }
}
