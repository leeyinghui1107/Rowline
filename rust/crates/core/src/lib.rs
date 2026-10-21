//! Rowline -- the core logic of the game.
//!
//! Rowline is an independent, clean-room re-implementation of the
//! behaviour of WinLinez v1.30 by Ivan Golubev; no original code or art
//! is used (see REPORT.md).
//!
//! This crate contains **only** behaviour: the board model, the rules, the
//! pseudo-random source and the client-area layout.  It performs no I/O,
//! touches no platform API and owns no global state, which is what makes it
//! usable from the desktop app, from the replay CLI and from tests alike.
//!
//! The authoritative description of every constant and rule lives in
//! `docs/03_FEATURE_SPEC.md`.  Where a rule is still uncertain the value is
//! exposed as a field on [`RuleModel`] instead of being hard-coded, so the
//! black-box phase can pin it down without touching call sites.
//!
//! ```
//! use rowline_core::{Engine, RuleModel};
//!
//! let mut eng = Engine::new(0xC0FFEE, RuleModel::default());
//! eng.new_game();
//! assert_eq!(eng.board().cells().len(), 81);
//! ```

/// The product's name.
///
/// The original game is called "WinLinez"; this re-implementation ships
/// under a name of its own, so that it can never be mistaken for the
/// original or for something the original author endorsed (REPORT.md,
/// rule R2).
pub const PRODUCT_NAME: &str = "Rowline";

pub mod board;
pub mod effects;
pub mod engine;
pub mod input;
pub mod layout;
pub mod menu;
pub mod render;
pub mod rng;
pub mod rules;
pub mod save;
pub mod scoreboard;
pub mod state;
pub mod trace;
pub mod view;

pub use board::{coords, index, Board, Cell, BOARD_CELLS, BOARD_SIDE, EMPTY};
pub use effects::{EffectSystem, Particle};
pub use engine::{Engine, MoveResult, MoveStatus};
pub use input::{InputEvent, Key, Modifiers, MouseButton, Point, Size};
pub use layout::Layout;
pub use menu::{Menu, MenuItem, MenuMetrics, MenuModel};
pub use render::{Color, FrameStyle, Rect, RenderCmd, SpriteSheetId};
pub use rng::Lcg;
pub use rules::RuleModel;
pub use save::{SaveData, SaveError, SaveFormat, ScoreRecord, Settings};
pub use scoreboard::{Players, Row, Scoreboard};
pub use state::{GameState, Phase};
pub use trace::{Op, Trace};
pub use view::build_frame;

/// Number of distinct ball colours.
pub const COLORS: u8 = 7;
/// Cells needed in a line to clear it.
pub const THRESHOLD: usize = 5;
