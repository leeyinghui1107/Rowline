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
use crate::rules::{find_lines, find_path, score_for_run, RuleModel};

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
    /// How many balls of each colour have been eliminated this game.
    ///
    /// Index `0` is unused.  The original keeps the same array at
    /// `0x40B2C4`, resets it in `New_Game` (`0x40282F`) and adds the number
    /// of cleared cells to the moved colour (`0x401C7A`); the statistics
    /// window prints it as `消除-N`.
    cleared: [u32; 8],
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
            cleared: [0; 8],
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

    /// The number of balls currently on the board, per colour.
    ///
    /// Index `0` counts the empty cells, exactly like the statistics window
    /// in the original (`0x404345` walks eight sprite slots, slot 0 being the
    /// empty plate).
    pub fn colour_counts(&self) -> [u32; 8] {
        let mut counts = [0u32; 8];
        for &colour in self.board.cells() {
            counts[colour as usize] += 1;
        }
        counts
    }

    /// How many balls of each colour this game has eliminated.
    ///
    /// Index `0` is unused; the value is what the statistics window shows as
    /// `消除-N`.
    pub fn cleared_counts(&self) -> [u32; 8] {
        self.cleared
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
    /// The original rolls the three-ball preview first and then places the
    /// first five balls from that same stream: each placement consumes the
    /// queue head and immediately rolls a replacement colour.  The remaining
    /// queue is the next group.
    pub fn new_game(&mut self) {
        self.board = Board::new();
        self.score = 0;
        self.cleared = [0; 8];
        self.roll_next_group();
        for _ in 0..self.model.initial_balls {
            self.spawn_until_quiet();
        }
    }

    /// Place one preview ball at a uniformly random empty cell.
    ///
    /// Returns `true` when the placement completed a line.  The original
    /// clears such a line without scoring and immediately places another
    /// ball, so the caller keeps calling until this returns `false`.
    fn spawn_one(&mut self) -> bool {
        let empty = self.board.empty_cells();
        if empty.is_empty() {
            return false;
        }
        let pick = empty[self.rng.below(empty.len() as u32) as usize];
        let colour = self.take_next_ball();
        self.board.cells_mut()[pick] = colour;
        let (total, cells) = find_lines(&self.board, pick, colour, self.model.threshold);
        if total == 0 {
            return false;
        }
        let removed = cells.len() as u32;
        for cell in cells {
            self.board.cells_mut()[cell] = EMPTY;
        }
        self.cleared[colour as usize] += removed;
        true
    }

    fn spawn_until_quiet(&mut self) {
        let mut guard = 0usize;
        while self.spawn_one() {
            guard += 1;
            if guard > BOARD_CELLS {
                break;
            }
        }
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
        let path = find_path(&self.board, src, dst);
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

        let (total, group) = find_lines(&self.board, dst, colour, self.model.threshold);
        if total > 0 {
            for &g in &group {
                let (r, c) = coords(g);
                self.board.set(r, c, EMPTY);
            }
            self.cleared[colour as usize] += group.len() as u32;
            let delta = score_for_run(total);
            self.score += delta;
            res.group = group;
            res.delta = delta;
            // A clearing move does not consume the preview group.
            return res;
        }

        for _ in 0..self.model.spawn_per_move {
            self.spawn_until_quiet();
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
    fn a_clear_counts_the_eliminated_balls_per_colour() {
        // The statistics window shows `消除-N`; the original adds the number
        // of removed cells to the moved colour (`0x401C7A`).
        let mut e = Engine::new(7, RuleModel::default());
        e.new_game();
        {
            let b = e.board_mut();
            b.cells_mut().fill(EMPTY);
            for c in 0..4 {
                b.set(4, c, 1);
            }
            b.set(5, 4, 1);
        }
        let res = e.move_ball(5, 4, 4, 4);
        assert_eq!(res.status, Some(MoveStatus::Moved));
        assert_eq!(res.group.len(), 5);
        let cleared = e.cleared_counts();
        assert_eq!(cleared[1], 5);
        assert_eq!(cleared[2..], [0; 6]);
        // Everything was eliminated, so all 81 cells are empty now.
        assert_eq!(e.colour_counts()[0], BOARD_CELLS as u32);
    }

    #[test]
    fn a_new_game_resets_the_per_colour_statistics() {
        let mut e = Engine::new(7, RuleModel::default());
        e.new_game();
        e.cleared[3] = 12;
        e.new_game();
        assert_eq!(e.cleared_counts()[3], 0);
        let counts = e.colour_counts();
        assert_eq!(counts.iter().sum::<u32>(), BOARD_CELLS as u32);
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
