//! Reordering items of an editable list.

/// Move the item at `idx` one place up. Returns whether anything moved
/// (false for the first item or an index out of range).
pub fn move_up<T>(list: &mut [T], idx: usize) -> bool {
    if idx == 0 || idx >= list.len() {
        return false;
    }
    list.swap(idx - 1, idx);
    true
}

/// Move the item at `idx` one place down. Returns whether anything moved
/// (false for the last item or an index out of range).
pub fn move_down<T>(list: &mut [T], idx: usize) -> bool {
    if idx + 1 >= list.len() {
        return false;
    }
    list.swap(idx, idx + 1);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn moving_swaps_with_the_neighbour() {
        let mut v = vec!["a", "b", "c"];
        assert!(move_up(&mut v, 1));
        assert_eq!(v, ["b", "a", "c"]);
        assert!(move_down(&mut v, 1));
        assert_eq!(v, ["b", "c", "a"]);
    }

    #[test]
    fn the_ends_and_bad_indexes_do_not_move() {
        let mut v = vec![1, 2, 3];
        assert!(!move_up(&mut v, 0));
        assert!(!move_down(&mut v, 2));
        assert!(!move_up(&mut v, 9));
        assert!(!move_down(&mut v, 9));
        assert_eq!(v, [1, 2, 3]);
        let mut empty: Vec<u8> = vec![];
        assert!(!move_up(&mut empty, 0));
        assert!(!move_down(&mut empty, 0));
    }
}
