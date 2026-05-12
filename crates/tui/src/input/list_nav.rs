//! Tiny helper for moving a `selected: usize` cursor in list-style dialogs
//! (Manage*, Import, GitLog, DispatchCardMove, …). Centralizes the
//! clamp/wrap arithmetic so handlers can express their j/k arms in one line.
//!
//! Not a trait: the survey of the 8 list-nav dialogs (see plan
//! `calidad-de-c-digo-elegant-dusk.md`) found that their commit keys and
//! side effects diverge too much to share more than this.

/// Move `selected` by `delta`, clamping at `[0, total-1]`. If `wrap` is true,
/// stepping past either end wraps around. Empty lists (`total == 0`) leave
/// `selected` at 0 without panicking.
pub(crate) fn move_selection(selected: &mut usize, total: usize, delta: isize, wrap: bool) {
    if total == 0 {
        *selected = 0;
        return;
    }
    let cur = (*selected).min(total - 1) as isize;
    let total_i = total as isize;
    let next = if wrap {
        // Two `% total_i` to handle negative results from the first one.
        ((cur + delta) % total_i + total_i) % total_i
    } else {
        (cur + delta).clamp(0, total_i - 1)
    };
    *selected = next as usize;
}

#[cfg(test)]
mod tests {
    use super::move_selection;

    #[test]
    fn empty_list_keeps_selected_zero() {
        let mut s = 0;
        move_selection(&mut s, 0, 1, false);
        assert_eq!(s, 0);
        move_selection(&mut s, 0, -1, true);
        assert_eq!(s, 0);
    }

    #[test]
    fn no_wrap_clamps_at_floor() {
        let mut s = 0;
        move_selection(&mut s, 5, -1, false);
        assert_eq!(s, 0);
    }

    #[test]
    fn no_wrap_clamps_at_ceiling() {
        let mut s = 4;
        move_selection(&mut s, 5, 1, false);
        assert_eq!(s, 4);
    }

    #[test]
    fn no_wrap_advances_normally() {
        let mut s = 2;
        move_selection(&mut s, 5, 1, false);
        assert_eq!(s, 3);
        move_selection(&mut s, 5, -1, false);
        assert_eq!(s, 2);
    }

    #[test]
    fn wrap_cycles_past_ceiling() {
        let mut s = 4;
        move_selection(&mut s, 5, 1, true);
        assert_eq!(s, 0);
    }

    #[test]
    fn wrap_cycles_past_floor() {
        let mut s = 0;
        move_selection(&mut s, 5, -1, true);
        assert_eq!(s, 4);
    }

    #[test]
    fn larger_delta_works_with_wrap() {
        let mut s = 0;
        move_selection(&mut s, 5, 7, true);
        assert_eq!(s, 2); // (0 + 7) % 5 = 2

        let mut s = 0;
        move_selection(&mut s, 5, -7, true);
        assert_eq!(s, 3); // (-7 % 5 + 5) % 5 = 3
    }

    #[test]
    fn page_jump_clamped_at_end() {
        let mut s = 1;
        move_selection(&mut s, 5, 10, false);
        assert_eq!(s, 4);

        let mut s = 3;
        move_selection(&mut s, 5, -10, false);
        assert_eq!(s, 0);
    }

    #[test]
    fn out_of_range_selected_is_clamped_before_move() {
        // If a caller stashed an out-of-range index (e.g. after filter change),
        // the next move starts from the clamped position, not the stale value.
        let mut s = 999;
        move_selection(&mut s, 5, -1, false);
        assert_eq!(s, 3); // clamped to 4, then -1
    }
}
