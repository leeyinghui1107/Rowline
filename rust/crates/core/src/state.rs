//! The game state: a read-only snapshot the UI and the renderer consume.
//!
//! The authoritative mutable state lives in [`crate::engine::Engine`].
//! `GameState` is what the engine *publishes* at the end of a tick, so that
//! no consumer can accidentally mutate the model.

use crate::board::{coords, Board, BOARD_SIDE};
use crate::engine::Engine;
use crate::layout::Layout;

/// The coarse phase of a game.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    /// Nothing shown yet.
    Boot,
    /// Waiting for the player.
    Ready,
    /// A ball is selected.
    Selected,
    /// An elimination animation is running.
    Animating,
    /// No legal move remains.
    GameOver,
    /// The save file failed its integrity check.
    Tampered,
}

/// Everything the presentation layers need to know.
#[derive(Clone, Debug)]
pub struct GameState {
    pub phase: Phase,
    pub score: i64,
    pub best_score: i64,
    pub board: Board,
    /// Index of the selected cell, if any.
    pub selected: Option<usize>,
    /// The path currently being previewed or animated.
    pub path: Vec<usize>,
    /// Whether the "next set" preview is visible.
    pub show_next: bool,
    /// Whether the statistics readout is visible.
    pub show_statistics: bool,
    /// Whether the polygon sprite set is in use.
    pub use_figures: bool,
    /// The balls the game is about to place (the "next group").
    ///
    /// The original previews three of them (docs/11 section 2.1).
    pub next_group: [u8; crate::engine::GROUP_SIZE],
    /// The integer scale of the loaded sprite sheet (1 = @1x, 2 = @2x, ...).
    ///
    /// The source rectangles in the render commands are expressed in the
    /// sheet's own texel space, so this is what keeps the sprite sharp on a
    /// high-DPI display.
    pub sheet_scale: u32,
    /// Resolved client-area geometry.
    pub layout: Layout,
}

impl GameState {
    /// Build a snapshot from the engine and the current layout.
    pub fn from_engine(engine: &Engine, layout: Layout) -> GameState {
        let phase = if engine.board().is_full() {
            Phase::GameOver
        } else {
            Phase::Ready
        };
        GameState {
            phase,
            score: engine.score(),
            best_score: 0,
            board: engine.board().clone(),
            selected: None,
            path: Vec::new(),
            show_next: true,
            show_statistics: true,
            use_figures: engine.use_figures(),
            next_group: engine.next_group(),
            sheet_scale: 1,
            layout,
        }
    }

    /// The colour of the cell at `(row, col)`, `0` when empty.
    pub fn colour_at(&self, row: usize, col: usize) -> u8 {
        self.board.colour_at(row, col)
    }

    /// The board side length.
    pub fn side(&self) -> usize {
        BOARD_SIDE
    }

    /// Human-readable position of a cell index.
    pub fn position(&self, cell: usize) -> (usize, usize) {
        coords(cell)
    }
}
