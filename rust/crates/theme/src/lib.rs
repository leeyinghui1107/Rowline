//! The design system for Rowline.
//!
//! This crate is the **single source of truth** for the visual language:
//!
//! * [`palette`] -- the Classic colour set;
//! * [`tokens`] -- the 8 pt spacing / radius grid and the type scale;
//! * [`contrast`] -- WCAG contrast maths, so accessibility is *verified*;
//! * [`theme`] -- the bundle that the renderer and the UI actually consume.
//!
//! It has **no dependencies** and performs no I/O, so it can be unit-tested
//! exhaustively and reused by any backend (wgpu, softbuffer, egui, web).
//!
//! ```
//! use rowline_theme::DesignSystem;
//!
//! let ds = DesignSystem::new();
//! assert!(ds.palette.surface_alt.to_hex() != 0);
//! ```

pub mod contrast;
pub mod palette;
pub mod theme;
pub mod tokens;

pub use contrast::{contrast_ratio, AA_TEXT};
pub use palette::{BallColors, Palette, Rgba};
pub use theme::DesignSystem;
pub use tokens::{Radius, Space, TextRole};
