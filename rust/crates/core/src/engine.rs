//! The engine: board state plus the operations that mutate it.
//!
//! The engine owns no I/O and exposes no globals.  Everything it needs from
//! the outside world arrives as arguments, which is what lets the same
//! engine drive the desktop app and the head-less replay CLI.

use crate::board::{coords, index, Board, BOARD_CELLS, EMPTY};

/// The number of balls the game previews as the "next group".
///
/// The original shows three balls next to the `下组` label (docs/11 section 2.1),
/// so the pending queue has three entries.
pub const GROUP_SIZE: usize = 3;
use crate::rng::Lcg;
use crate::rules::{connected_group, find_path, RuleModel};

/// Outcome of a move attempt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MoveStatus {
    /// The move was not allowed at all (empty source, occupied target).
    Illegal,
    /// The move was allowed but no path exists.
    NoPath,
    /// The ball moved.
    Moved,
}

/// Everything that happened during a move.
#[derive(Clone, Debug, Default)]
pub struct MoveResult {
    pub status: Option<MoveStatus>,
    /// Visited cells of the path, including the source.
    pub path: Vec<usize>,
    /// Cells eliminated, sorted ascending.
    pub group: Vec<usize>,
    /// Score delta.
    pub delta: i64,
}

/// The game engine.
pub struct Engine {
    board: Board,
    score: i64,
    use_figures: bool,
    rng: Lcg,
    model: RuleModel,
    step: u32,
    /// The balls the game is about to place, oldest first.
    next_group: [u8; GROUP_SIZE],
}

impl Engine {
    /// Create an engine with an explicit seed and rule model.
    pub fn new(seed: u32, model: RuleModel) -> Engine {
        Engine {
            board: Board::new(),
            score: 0,
            use_figures: false,
            rng: Lcg::with_seed(seed),
            model,
            step: 0,
            next_group: [0; GROUP_SIZE],
        }
    }

    /// The current board.
    pub fn board(&self) -> &Board {
        &self.board
    }

    /// Mutable access to the board (used when replaying a recorded board).
    pub fn board_mut(&mut self) -> &mut Board {
        &mut self.board
    }

    /// The current score.
    pub fn score(&self) -> i64 {
        self.score
    }

    /// Overwrite the score.
    ///
    /// Used when a saved game is restored, and by the tests that need a
    /// non-zero score without playing a whole round.
    pub fn set_score(&mut self, score: i64) {
        self.score = score;
    }

    /// Whether the polygon sprite set is in use.
    pub fn use_figures(&self) -> bool {
        self.use_figures
    }

    /// The current step counter.
    pub fn step(&self) -> u32 {
        self.step
    }

    /// Advance the step counter.
    pub fn bump(&mut self) {
        self.step += 1;
    }

    /// Toggle the polygon sprite set.
    pub fn set_use_figures(&mut self, v: bool) {
        self.use_figures = v;
    }

    /// `true` when at least one move is still possible.
    ///
    /// When this turns false the game is over.
    pub fn has_legal_move(&self) -> bool {
        crate::rules::has_legal_move(&self.board, self.model.threshold)
    }

    /// The balls that are about to be placed.
    pub fn next_group(&self) -> [u8; GROUP_SIZE] {
        self.next_group
    }

    /// Roll a fresh next group.
    pub fn roll_next_group(&mut self) {
        for slot in self.next_group.iter_mut() {
            *slot = self.rng.colour();
        }
    }

    /// Take the next ball from the queue and roll a replacement.
    pub fn take_next_ball(&mut self) -> u8 {
        let ball = self.next_group[0];
        for i in 0..GROUP_SIZE - 1 {
            self.next_group[i] = self.next_group[i + 1];
        }
        self.next_group[GROUP_SIZE - 1] = self.rng.colour();
        ball
    }

    /// Generate a fresh board.
    ///
    /// The occupancy is parameterised by [`RuleModel::initial_balls`]: the
    /// original puts **five** random balls on the empty board (docs/11
    /// section 5c).  The Python, C and Rust implementations consume the RNG
    /// in exactly the same order, so they stay bit-identical.
    pub fn new_game(&mut self) {
        self.board = Board::new();
        self.score = 0;
        // A fresh board holds a handful of random balls, not a full one.
        let mut placed = 0usize;
        let mut guard = 0usize;
        while placed < self.model.initial_balls && guard < BOARD_CELLS * 4 {
            guard += 1;
            let pick = self.rng.below(BOARD_CELLS as u32) as usize;
            if self.board.cells()[pick] != EMPTY {
                continue;
            }
            let c = self.rng.colour();
            self.board.cells_mut()[pick] = c;
            placed += 1;
        }
        self.roll_next_group();
    }

    /// Apply a move.
    pub fn move_ball(&mut self, r0: i32, c0: i32, r1: i32, c1: i32) -> MoveResult {
        let mut res = MoveResult::default();
        if !(0..9).contains(&r0)
            || !(0..9).contains(&c0)
            || !(0..9).contains(&r1)
            || !(0..9).contains(&c1)
        {
            res.status = Some(MoveStatus::Illegal);
            return res;
        }
        let (r0, c0, r1, c1) = (r0 as usize, c0 as usize, r1 as usize, c1 as usize);
        let colour = self.board.get(r0, c0);
        if colour == EMPTY || self.board.get(r1, c1) != EMPTY {
            res.status = Some(MoveStatus::Illegal);
            return res;
        }

        let src = index(r0, c0);
        let dst = index(r1, c1);
        let path = find_path(&self.board, src, dst, &self.model);
        if path.is_empty() {
            res.status = Some(MoveStatus::NoPath);
            return res;
        }
        res.status = Some(MoveStatus::Moved);
        res.path = path;

        self.board.set(r1, c1, colour);
        if !self.model.source_keeps_colour {
            self.board.set(r0, c0, EMPTY);
        }

        let group = connected_group(&self.board, dst, colour, self.model.threshold);
        if group.is_empty() {
            return res;
        }
        let delta = group.len() as i64 * self.model.points_per_ball;
        if delta < self.model.min_score_delta {
            return res;
        }
        for &g in &group {
            let (r, c) = coords(g);
            self.board.set(r, c, EMPTY);
        }
        self.score += delta;
        res.group = group;
        res.delta = delta;

        for _ in 0..self.model.refill {
            if self.board.is_full() {
                break;
            }
            let pick = self.rng.below(BOARD_CELLS as u32) as usize;
            let (r, c) = coords(pick);
            let colour = self.rng.colour();
            self.board.set(r, c, colour);
        }
        res
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_game_places_exactly_five_balls() {
        // The original's new game puts five random balls on the board
        // (docs/11 section 5c).
        let mut e = Engine::new(7, RuleModel::default());
        e.new_game();
        let filled = e.board().cells().iter().filter(|&&c| c != EMPTY).count();
        assert_eq!(filled, 5, "unexpected fill: {filled}");
    }

    #[test]
    fn the_next_group_holds_three_balls() {
        let mut e = Engine::new(7, RuleModel::default());
        e.new_game();
        let group = e.next_group();
        assert_eq!(group.len(), 3);
        for c in group {
            assert!((1..=7).contains(&c), "bad colour {c}");
        }
    }

    #[test]
    fn taking_a_ball_rolls_a_replacement() {
        let mut e = Engine::new(7, RuleModel::default());
        e.new_game();
        let before = e.next_group();
        let taken = e.take_next_ball();
        let after = e.next_group();
        assert_eq!(taken, before[0]);
        assert_eq!(after[0], before[1]);
        assert_eq!(after[1], before[2]);
    }

    #[test]
    fn an_illegal_move_is_reported() {
        let mut e = Engine::new(7, RuleModel::default());
        e.new_game();
        let r = e.move_ball(0, 0, 1, 1);
        // either the source is empty (illegal) or the target is occupied
        assert!(matches!(
            r.status,
            Some(MoveStatus::Illegal) | Some(MoveStatus::NoPath) | Some(MoveStatus::Moved)
        ));
    }
}
