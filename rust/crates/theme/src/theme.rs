//! The bundle the UI and the renderer consume.

use crate::motion::{Duration, Easing, Motion};
use crate::palette::{Palette, Rgba, ThemeKind};
use crate::tokens::{DesignTokens, Radius, Space, TextRole};

/// The pointer state of an interactive element.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HitState {
    Idle,
    Hover,
    Press,
    Disabled,
}

/// A resolved interaction state, with the animation progress included.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Interaction {
    pub state: HitState,
    /// The 0..1 progress of the hover transition.
    pub hover: f32,
    /// The 0..1 progress of the press transition.
    pub press: f32,
}

impl Interaction {
    /// A fresh, idle interaction.
    pub fn idle() -> Self {
        Interaction {
            state: HitState::Idle,
            hover: 0.0,
            press: 0.0,
        }
    }

    /// The background colour of a button in this state.
    pub fn button_surface(&self, p: &Palette) -> Rgba {
        match self.state {
            HitState::Idle => p.surface_raised,
            HitState::Hover => p.surface_hover,
            HitState::Press => p.surface_active,
            HitState::Disabled => p.surface_alt,
        }
    }
}

/// Everything the UI needs, in one value.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DesignSystem {
    pub palette: Palette,
    pub tokens: DesignTokens,
    pub motion: Motion,
}

impl DesignSystem {
    /// A design system for a theme, honouring the reduced-motion preference.
    pub fn new(kind: ThemeKind, reduced_motion: bool) -> Self {
        DesignSystem {
            palette: Palette::of(kind),
            tokens: DesignTokens::default(),
            motion: Motion::new(reduced_motion),
        }
    }

    /// Switch the theme, keeping the preferences.
    pub fn with_theme(mut self, kind: ThemeKind) -> Self {
        self.palette = Palette::of(kind);
        self
    }

    /// Switch the theme.
    pub fn set_theme(&mut self, kind: ThemeKind) {
        self.palette = Palette::of(kind);
    }

    /// The theme kind.
    pub fn kind(&self) -> ThemeKind {
        self.palette.kind
    }

    /// Enable or disable the reduced-motion preference.
    pub fn set_reduced_motion(&mut self, reduced: bool) {
        self.motion = Motion::new(reduced);
    }

    /// The user's text size preference.
    pub fn set_text_scale(&mut self, scale: crate::tokens::TextScale) {
        self.tokens.text_scale = scale;
    }

    /// The size of a text role, after the user's preference is applied.
    pub fn text_size(&self, role: TextRole) -> f32 {
        role.size() * self.tokens.text_scale.factor()
    }

    /// The line height of a text role, after the user's preference.
    pub fn text_line_height(&self, role: TextRole) -> f32 {
        role.line_height() * self.tokens.text_scale.factor()
    }

    /// The progress of a transition that started `elapsed` milliseconds ago.
    pub fn progress(&self, elapsed: u32, d: Duration, easing: Easing) -> f32 {
        self.motion.progress(elapsed, d, easing)
    }

    /// The spacing step.
    pub fn space(&self, s: Space) -> f32 {
        s.px()
    }

    /// The corner radius.
    pub fn radius(&self, r: Radius) -> f32 {
        r.px()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tokens::TextScale;

    #[test]
    fn the_theme_can_be_switched_without_losing_preferences() {
        let mut ds = DesignSystem::new(ThemeKind::Dark, true);
        ds.set_text_scale(TextScale::Large);
        ds.set_theme(ThemeKind::Light);
        assert_eq!(ds.kind(), ThemeKind::Light);
        assert!(ds.motion.reduced);
        assert_eq!(ds.tokens.text_scale, TextScale::Large);
    }

    #[test]
    fn the_text_scale_actually_scales_the_type() {
        let mut ds = DesignSystem::new(ThemeKind::Dark, false);
        let normal = ds.text_size(TextRole::Body);
        ds.set_text_scale(TextScale::Large);
        assert!(ds.text_size(TextRole::Body) > normal);
        assert!((ds.text_size(TextRole::Body) / normal - 1.125).abs() < 1e-4);
    }

    #[test]
    fn button_surfaces_differ_per_state() {
        let ds = DesignSystem::new(ThemeKind::Dark, false);
        let p = &ds.palette;
        let idle = Interaction {
            state: HitState::Idle,
            hover: 0.0,
            press: 0.0,
        }
        .button_surface(p);
        let hover = Interaction {
            state: HitState::Hover,
            hover: 1.0,
            press: 0.0,
        }
        .button_surface(p);
        let press = Interaction {
            state: HitState::Press,
            hover: 1.0,
            press: 1.0,
        }
        .button_surface(p);
        assert_ne!(idle, hover);
        assert_ne!(hover, press);
    }
}
