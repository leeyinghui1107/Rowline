//! The game state: a read-only snapshot the UI and the renderer consume.
//!
//! The authoritative mutable state lives in [`crate::engine::Engine`].
//! `GameState` is what the engine *publishes* at the end of a tick, so that
//! no consumer can accidentally mutate the model.

use crate::board::{coords, Board, BOARD_SIDE};
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
