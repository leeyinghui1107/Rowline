//! Rowline -- the game itself, as a library.
//!
//! The binary in `main.rs` is a thin shell around this crate: it owns the
//! window, the display policy and the frame loop, while the game logic --
//! the state machine, the input handling and the drawing of a frame --
//! lives here.  Keeping it in a library means the **offline screenshot
//! tool** can render a frame without a window, which is what makes the
//! visual result verifiable on a build machine.

pub mod commands;
pub mod game;
