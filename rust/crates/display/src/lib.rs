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
//! On top of that sits a second decision: should the content fill the
//! window ([`ScalePolicy::Logical`]) or should it keep a fixed logical
//! resolution and be scaled to fit ([`ScalePolicy::Fixed`])?
//!
//! Everything in this crate is pure arithmetic: no I/O, no GPU, no window
//! system.  That is what makes the whole adaptation matrix testable in CI.

pub mod resample;
pub mod viewport;

pub use resample::{lanczos3_rgba, resample_rgba, select_variant};
pub use viewport::{compute_viewport, Rect, ScalePolicy, Size, Viewport};
