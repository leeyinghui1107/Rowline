//! The rules: path search, straight-line detection, scoring and game over.
//!
//! These semantics were recovered from WinLinez v1.30's binary and confirmed
//! with black-box runs (docs/11, `blackbox/checklist.json`):
//!
//! * a ball moves to any empty cell in its 4-connected empty region;
//! * the source cell becomes empty;
//! * a line is a straight run of at least five same-coloured balls along a
//!   row, a column, or one of the two diagonals -- a bent blob does not clear;
//! * a clearing turn does not consume the three-ball preview; a quiet turn
//!   places the preview balls;
//! * the score table is 10, 12, 18, 28, 42, ... for runs of 5, 6, 7, 8, 9.

use crate::board::{coords, in_bounds, index, Board, BOARD_CELLS, EMPTY};

/// The parts of the rule model that are useful for experiments.  The
/// original's confirmed values are the defaults; there is no linear
/// points-per-ball knob any more because the score table is non-linear.
#[derive(Clone, Copy, Debug)]
pub struct RuleModel {
    /// Cells needed in a straight run to clear it.
    pub threshold: usize,
    /// Balls placed after a turn that creates no line.
    pub spawn_per_move: usize,
    /// How many balls a fresh board starts with.
    pub initial_balls: usize,
    /// Experiments only; the original always empties the source cell.
    pub source_keeps_colour: bool,
}

impl Default for RuleModel {
    fn default() -> Self {
        RuleModel {
            threshold: crate::THRESHOLD,
            spawn_per_move: crate::engine::GROUP_SIZE,
            initial_balls: 5,
            source_keeps_colour: false,
        }
    }
}

/// The four axes scanned by the original's line detector (each axis is
/// scanned in both directions): vertical, main diagonal, horizontal,
/// anti-diagonal.  Order recovered from the binary at `0x40A000`.
pub const AXES: [(isize, isize); 4] = [(-1, 0), (-1, -1), (0, -1), (1, -1)];

/// 4-neighbour step order used by the path search.
pub const STEPS4: [(isize, isize); 4] = [(-1, 0), (1, 0), (0, -1), (0, 1)];

/// Every straight run through `start` of at least `threshold` cells.
///
/// Returns `(total, cells)`: `total` is the sum of the qualifying run
/// lengths (the original's score function receives this sum), and `cells`
/// is the sorted union of the marked cells.
pub fn find_lines(
    board: &Board,
    start: usize,
    colour: u8,
    threshold: usize,
) -> (usize, Vec<usize>) {
    if board.cells()[start] != colour {
        return (0, Vec::new());
    }
    let (r0, c0) = coords(start);
    let mut marked = [false; BOARD_CELLS];
    let mut total = 0usize;
    for &(dr, dc) in &AXES {
        let mut run = vec![start];
        for sign in [1isize, -1] {
            let mut r = r0 as isize + sign * dr;
            let mut c = c0 as isize + sign * dc;
            while in_bounds(r, c) && board.get(r as usize, c as usize) == colour {
                run.push(index(r as usize, c as usize));
                r += sign * dr;
                c += sign * dc;
            }
        }
        if run.len() >= threshold {
            total += run.len();
            for &cell in &run {
                marked[cell] = true;
            }
        }
    }
    if total == 0 {
        return (0, Vec::new());
    }
    let cells = (0..BOARD_CELLS).filter(|&i| marked[i]).collect();
    (total, cells)
}

/// Score for a clear whose qualifying run lengths sum to `total`.
///
/// Recovered from `Auto+0x1e1d`: 5 -> 10, 6 -> 12, 7 -> 18, 8 -> 28, 9 -> 42.
pub fn score_for_run(total: usize) -> i64 {
    if total < crate::THRESHOLD {
        return 0;
    }
    let mut score = 10i64;
    let mut step = 2i64;
    for _ in 0..(total - crate::THRESHOLD) {
        score += step;
        step += 4;
    }
    score
}

/// Shortest 4-connected path from `src` to `dst` through empty cells.
///
/// The returned vector includes the source and the destination.  An empty
/// vector means the destination is occupied or unreachable.
pub fn find_path(board: &Board, src: usize, dst: usize) -> Vec<usize> {
    if src == dst || board.cells()[dst] != EMPTY {
        return Vec::new();
    }
    let mut parent = [usize::MAX; BOARD_CELLS];
    let mut seen = [false; BOARD_CELLS];
    seen[src] = true;
    let mut queue = std::collections::VecDeque::new();
    queue.push_back(src);
    while let Some(cur) = queue.pop_front() {
        if cur == dst {
            break;
        }
        let (r, c) = coords(cur);
        for &(dr, dc) in &STEPS4 {
            let nr = r as isize + dr;
            let nc = c as isize + dc;
            if !in_bounds(nr, nc) {
                continue;
            }
            let n = index(nr as usize, nc as usize);
            if seen[n] {
                continue;
            }
            if n != dst && board.cells()[n] != EMPTY {
                continue;
            }
            seen[n] = true;
            parent[n] = cur;
            queue.push_back(n);
        }
    }
    if !seen[dst] {
        return Vec::new();
    }
    let mut path = Vec::new();
    let mut cur = dst;
    loop {
        path.push(cur);
        if cur == src {
            break;
        }
        cur = parent[cur];
    }
    path.reverse();
    path
}

/// `true` while the round can continue.
///
/// The original only ends the game when the board is full after the refill
/// (`Auto+0x403301`: `Game_CountEmpty` == 0 -> game-over dialog).  It does
/// not test whether a line can still be completed.
pub fn has_legal_move(board: &Board, _threshold: usize) -> bool {
    !board.is_full()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_straight_five_is_found_and_sorted() {
        let mut b = Board::new();
        for c in 0..5 {
            b.set(4, c, 3);
        }
        let (total, cells) = find_lines(&b, index(4, 2), 3, 5);
        assert_eq!(total, 5);
        assert_eq!(
            cells,
            vec![
                index(4, 0),
                index(4, 1),
                index(4, 2),
                index(4, 3),
                index(4, 4)
            ]
        );
    }

    #[test]
    fn a_straight_four_is_below_the_threshold() {
        let mut b = Board::new();
        for c in 0..4 {
            b.set(4, c, 3);
        }
        assert_eq!(find_lines(&b, index(4, 1), 3, 5), (0, Vec::new()));
    }

    #[test]
    fn a_bent_five_does_not_clear() {
        // Four along the row plus one below the end: connected, but no
        // straight run of five.
        let mut b = Board::new();
        for c in 0..4 {
            b.set(4, c, 3);
        }
        b.set(5, 0, 3);
        assert_eq!(find_lines(&b, index(5, 0), 3, 5), (0, Vec::new()));
    }

    #[test]
    fn a_bridged_six_scores_twelve() {
        let mut b = Board::new();
        // 3 . 2 -> placing the gap makes six in a row.
        for c in 0..3 {
            b.set(0, c, 3);
        }
        b.set(0, 4, 3);
        b.set(0, 5, 3);
        b.set(0, 3, 3);
        let (total, cells) = find_lines(&b, index(0, 3), 3, 5);
        assert_eq!(total, 6);
        assert_eq!(cells.len(), 6);
        assert_eq!(score_for_run(total), 12);
    }

    #[test]
    fn the_score_table_matches_the_binary() {
        assert_eq!(score_for_run(4), 0);
        assert_eq!(score_for_run(5), 10);
        assert_eq!(score_for_run(6), 12);
        assert_eq!(score_for_run(7), 18);
        assert_eq!(score_for_run(8), 28);
        assert_eq!(score_for_run(9), 42);
    }

    #[test]
    fn an_occupied_destination_has_no_path() {
        let mut b = Board::new();
        b.set(0, 1, 4);
        assert!(find_path(&b, index(0, 0), index(0, 1)).is_empty());
    }

    #[test]
    fn a_diagonal_gap_is_not_a_path() {
        // (0,0) and (1,1) touch diagonally but the orthogonal route is
        // blocked, so the 4-connected search must fail.
        let mut b = Board::new();
        b.set(0, 1, 1);
        b.set(1, 0, 1);
        assert!(find_path(&b, index(0, 0), index(1, 1)).is_empty());
    }

    #[test]
    fn a_shortest_orthogonal_path_is_found() {
        let b = Board::new();
        let path = find_path(&b, index(0, 0), index(2, 2));
        assert_eq!(path.len(), 5);
        assert_eq!(path.first(), Some(&index(0, 0)));
        assert_eq!(path.last(), Some(&index(2, 2)));
    }

    #[test]
    fn a_full_board_is_game_over() {
        let mut b = Board::new();
        for i in 0..BOARD_CELLS {
            b.cells_mut()[i] = 1;
        }
        assert!(!has_legal_move(&b, 5));
    }
}
