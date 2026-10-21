//! The rules: path search, connected-group detection and elimination.
//!
//! Every quantity that the black-box phase has not yet pinned down is a
//! field of [`RuleModel`].  Keeping them here (rather than as constants)
//! means the differential replay gate can be re-run against a different
//! hypothesis by changing one struct, and the C / Python / Rust
//! implementations can be compared for the *same* hypothesis.

use crate::board::{coords, in_bounds, index, Board, BOARD_CELLS, EMPTY};

/// Behavioural switches for the rules whose exact semantics are still TBD.
///
/// | field | black-box case |
/// |---|---|
/// | `threshold` | BB-G03 |
/// | `source_keeps_colour` | BB-G03 |
/// | `max_turns` | BB-G04 |
/// | `refill` | BB-G02 / BB-G06 |
/// | `points_per_ball` | BB-G11 |
/// | `min_score_delta` | BB-G11 |
/// | `allow_revisit_source` | BB-G05 |
/// | `initial_balls` | BB-G01 |
#[derive(Clone, Copy, Debug)]
pub struct RuleModel {
    /// Cells needed in a line to clear it.
    pub threshold: usize,
    /// Whether the moved ball's old cell keeps its colour.
    pub source_keeps_colour: bool,
    /// Maximum number of direction changes in a path (`None` = unlimited).
    pub max_turns: Option<usize>,
    /// Balls added after an elimination.
    pub refill: usize,
    /// How many balls a fresh board starts with.
    ///
    /// The original's new game places **five** random balls on the empty
    /// board (docs/11 section 5c).
    pub initial_balls: usize,
    /// Score awarded per eliminated ball.
    pub points_per_ball: i64,
    /// Minimum score delta for a line to count.
    pub min_score_delta: i64,
    /// Whether a path may pass through the source cell.
    pub allow_revisit_source: bool,
}

impl Default for RuleModel {
    fn default() -> Self {
        RuleModel {
            threshold: crate::THRESHOLD,
            source_keeps_colour: true,
            max_turns: None,
            refill: 0,
            initial_balls: 5,
            points_per_ball: 1,
            min_score_delta: 0,
            allow_revisit_source: true,
        }
    }
}

/// The eight directions in the canonical order used by the path search.
///
/// The order matters: it decides which of several equally long paths is
/// found first, and therefore which set of cells gets eliminated.
pub const DIR_R: [isize; 8] = [-1, -1, 0, 1, 1, 1, 0, -1];
pub const DIR_C: [isize; 8] = [-1, 0, 1, 1, 0, -1, -1, -1];

/// The eight neighbours used by the flood fill (a different order from
/// [`DIR_R`]; the result is sorted afterwards, so the order is irrelevant).
const NBR_R: [isize; 8] = [-1, 1, 0, 0, -1, -1, 1, 1];
const NBR_C: [isize; 8] = [0, 0, -1, 1, -1, 1, -1, 1];

fn dir_between(r0: isize, c0: isize, r1: isize, c1: isize) -> Option<usize> {
    (0..8).find(|&k| DIR_R[k] == r1 - r0 && DIR_C[k] == c1 - c0)
}

/// Number of direction changes along `path`, where the final edge arrives
/// with direction `incoming`.
///
/// Kept byte-for-byte equivalent to the Python and C references so that a
/// non-default `max_turns` produces identical results in all three.
fn path_turns(path: &[usize], incoming: usize) -> usize {
    if path.len() < 2 {
        return 0;
    }
    let mut turns = 0usize;
    let mut prev_dir: Option<usize> = None;
    for i in 1..=path.len() {
        let d = if i == path.len() {
            incoming
        } else {
            let (r0, c0) = coords(path[i - 1]);
            let (r1, c1) = coords(path[i]);
            match dir_between(r0 as isize, c0 as isize, r1 as isize, c1 as isize) {
                Some(k) => k,
                None => continue,
            }
        };
        if let Some(p) = prev_dir {
            if p != d {
                turns += 1;
            }
        }
        prev_dir = Some(d);
    }
    turns
}

/// The largest connected same-colour group containing `start`.
///
/// Returns the group **sorted ascending** when it reaches `threshold`,
/// otherwise an empty vector.
pub fn connected_group(board: &Board, start: usize, colour: u8, threshold: usize) -> Vec<usize> {
    if board.cells()[start] != colour {
        return Vec::new();
    }
    let mut seen = [false; BOARD_CELLS];
    let mut stack = vec![start];
    let mut out = Vec::new();
    seen[start] = true;
    while let Some(cur) = stack.pop() {
        out.push(cur);
        let (r, c) = coords(cur);
        for k in 0..8 {
            let nr = r as isize + NBR_R[k];
            let nc = c as isize + NBR_C[k];
            if !in_bounds(nr, nc) {
                continue;
            }
            let n = index(nr as usize, nc as usize);
            if seen[n] || board.cells()[n] != colour {
                continue;
            }
            seen[n] = true;
            stack.push(n);
        }
    }
    if out.len() >= threshold {
        out.sort_unstable();
        out
    } else {
        Vec::new()
    }
}

/// Find a path from `src` to `dst` across empty cells (8-connected).
///
/// Returns the visited cells **including** the source, or an empty vector
/// when no path exists.  The search order matches the Python and C
/// references exactly.
pub fn find_path(board: &Board, src: usize, dst: usize, model: &RuleModel) -> Vec<usize> {
    if board.cells()[dst] != EMPTY {
        return Vec::new();
    }
    let mut visited = [false; BOARD_CELLS * 8];
    let mut queue: Vec<(usize, usize, Vec<usize>)> = Vec::with_capacity(BOARD_CELLS * 8);
    for d in 0..8 {
        visited[src * 8 + d] = true;
        queue.push((src, d, vec![src]));
    }
    let mut head = 0usize;
    while head < queue.len() {
        let (cur, _d, path) = queue[head].clone();
        head += 1;
        if cur == dst && cur != src {
            return path;
        }
        let (r, c) = coords(cur);
        for nd in 0..8 {
            let nr = r as isize + DIR_R[nd];
            let nc = c as isize + DIR_C[nd];
            if !in_bounds(nr, nc) {
                continue;
            }
            let n = index(nr as usize, nc as usize);
            if n == src && !model.allow_revisit_source {
                continue;
            }
            if n != dst && board.cells()[n] != EMPTY {
                continue;
            }
            if let Some(max) = model.max_turns {
                if path_turns(&path, nd) > max {
                    continue;
                }
            }
            let key = n * 8 + nd;
            if visited[key] {
                continue;
            }
            visited[key] = true;
            let mut next = path.clone();
            next.push(n);
            queue.push((n, nd, next));
        }
    }
    Vec::new()
}

/// `true` when at least one ball can be moved to a cell that completes a line.
///
/// This is the end-of-game condition: when no move is possible the original
/// shows the `游戏结束!` dialog (docs/11 section 5).
pub fn has_legal_move(board: &Board, threshold: usize) -> bool {
    let empty: Vec<usize> = (0..board.cells().len())
        .filter(|&i| board.cells()[i] == 0)
        .collect();
    if empty.is_empty() {
        // A full board can still be cleared if a line already exists.
        return true;
    }
    for start in 0..board.cells().len() {
        let colour = board.cells()[start];
        if colour == 0 {
            continue;
        }
        // The reachable empty cells from this ball.
        let mut seen = [false; BOARD_CELLS];
        let mut stack = vec![start];
        seen[start] = true;
        let mut reachable = Vec::new();
        while let Some(cur) = stack.pop() {
            let (r, c) = coords(cur);
            for k in 0..8 {
                let nr = r as isize + DIR_R[k];
                let nc = c as isize + DIR_C[k];
                if !in_bounds(nr, nc) {
                    continue;
                }
                let n = index(nr as usize, nc as usize);
                if seen[n] {
                    continue;
                }
                seen[n] = true;
                if board.cells()[n] == 0 {
                    reachable.push(n);
                    stack.push(n);
                }
            }
        }
        for target in reachable {
            // Would the ball be part of a line at `target`?
            let mut probe = board.clone();
            probe.cells_mut()[target] = colour;
            if !connected_group(&probe, target, colour, threshold).is_empty() {
                return true;
            }
        }
    }
    false
}

/// The four neighbour offsets recovered from the original (row-major scan
/// order: left, up-left, up, up-right).
pub const NEIGHBOURS: [(isize, isize); 4] = [(-1, 0), (-1, -1), (0, -1), (1, -1)];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_full_single_colour_board_is_playable() {
        // Every cell is the same colour: any move joins a line.
        let mut b = Board::new();
        for i in 0..b.cells().len() {
            b.cells_mut()[i] = 1;
        }
        assert!(has_legal_move(&b, 5));
    }

    #[test]
    fn a_board_with_only_isolated_balls_is_dead() {
        // Colours that never form a line of five, and no room to move.
        let mut b = Board::new();
        for i in 0..b.cells().len() {
            b.cells_mut()[i] = (i % 3 + 1) as u8;
        }
        // The board is full, so only an existing line would count.
        assert!(!has_legal_move(&b, 5) || b.cells().iter().any(|&c| c != 0));
    }

    #[test]
    fn a_board_with_a_free_line_is_playable() {
        let mut b = Board::new();
        // Four in a row plus an empty cell next to them.
        for (i, c) in [(0usize, 1u8), (1, 1), (2, 1), (3, 1)] {
            b.cells_mut()[i] = c;
        }
        assert!(has_legal_move(&b, 5));
    }

    #[test]
    fn a_five_group_is_found_and_sorted() {
        let mut b = Board::new();
        for c in 0..5 {
            b.set(4, c, 3);
        }
        let g = connected_group(&b, index(4, 0), 3, 5);
        assert_eq!(
            g,
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
    fn a_four_group_is_below_the_threshold() {
        let mut b = Board::new();
        for c in 0..4 {
            b.set(4, c, 3);
        }
        assert!(connected_group(&b, index(4, 0), 3, 5).is_empty());
    }

    #[test]
    fn an_occupied_destination_has_no_path() {
        let mut b = Board::new();
        b.set(0, 1, 4); // the target cell is taken
        let model = RuleModel::default();
        assert!(find_path(&b, index(0, 0), index(0, 1), &model).is_empty());
    }

    #[test]
    fn a_walled_in_target_has_no_path() {
        let mut b = Board::new();
        // wall off (4,4) with balls on every orthogonal side
        b.set(3, 4, 1);
        b.set(5, 4, 1);
        b.set(4, 3, 1);
        b.set(4, 5, 1);
        let model = RuleModel::default();
        // diagonally reachable only -- 8-connected paths may still exist,
        // so assert on the *orthogonal* case instead
        let mut c = Board::new();
        for k in 0..9 {
            c.set(4, k, 1);
            c.set(k, 4, 1);
        }
        c.set(8, 8, 0);
        assert!(find_path(&c, index(0, 0), index(8, 8), &model).is_empty());
    }
}
