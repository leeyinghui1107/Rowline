//! Display adaptation: the logical/physical coordinate split and the viewport
//! that maps one onto the other.
//!
//! Two things have to be kept apart:
//!
//! * the **logical** coordinate system, in which the game is laid out.  One
//!   logical unit is one "CSS pixel": on a 200% display it covers two
//!   physical pixels.  All layout maths happens here, so the board is 36
//!   logical pixels wide regardless of the display;
//! * the **physical** coordinate system, in which the GPU works.  This is
//!   what the swap chain is sized in.
//!
//! The content always fills the window; the layout is recomputed for the
//! client size.  There is deliberately no separate fixed-resolution mode:
//! the board keeps its calibrated 36-logical-pixel cells and the window
//! around it simply gains or loses margin.
//!
//! Everything in this crate is pure arithmetic: no I/O, no GPU, no window
//! system.  That is what makes the whole adaptation matrix testable in CI.

pub mod resample;
pub mod viewport;

pub use resample::select_variant;
pub use viewport::{compute_viewport, Rect, Size, Viewport};
