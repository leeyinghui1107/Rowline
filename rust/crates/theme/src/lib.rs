//! The design system for Rowline.
//!
//! This crate is the **single source of truth** for the visual language:
//!
//! * [`palette`] -- the light and dark colour sets;
//! * [`tokens`] -- the 8 pt spacing / radius / shadow grid and the type scale;
//! * [`motion`] -- easing curves, durations and the reduced-motion switch;
//! * [`contrast`] -- WCAG contrast maths, so accessibility is *verified*;
//! * [`theme`] -- the bundle that the renderer and the UI actually consume.
//!
//! It has **no dependencies** and performs no I/O, so it can be unit-tested
//! exhaustively and reused by any backend (wgpu, softbuffer, egui, web).
//!
//! ```
//! use rowline_theme::{DesignSystem, ThemeKind};
//!
//! let ds = DesignSystem::new(ThemeKind::Dark, false);
//! assert!(ds.palette.text_primary.to_hex() != 0);
//! ```

pub mod contrast;
pub mod motion;
pub mod palette;
pub mod theme;
pub mod tokens;

pub use contrast::{contrast_ratio, AA_LARGE_TEXT, AA_NON_TEXT, AA_TEXT};
pub use motion::{Duration, Easing, Motion};
pub use palette::{BallColors, Palette, Rgba, ThemeKind};
pub use theme::{DesignSystem, HitState, Interaction};
pub use tokens::{DesignTokens, Radius, Shadow, ShadowStep, Space, TextRole, TextScale};
