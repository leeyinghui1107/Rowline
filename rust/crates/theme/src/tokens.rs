//! Spacing, radii, shadows and the type scale.
//!
//! Everything is on an **8 pt grid** (with 4 pt as the half-step), so that
//! the layout maths is trivial and nothing drifts by a pixel.

/// A spacing step. The value is the size in logical pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Space {
    None,
    Xs,
    Sm,
    Md,
    Lg,
    Xl,
    Xxl,
    Huge,
}

impl Space {
    /// The size in logical pixels.
    pub const fn px(self) -> f32 {
        match self {
            Space::None => 0.0,
            Space::Xs => 4.0,
            Space::Sm => 8.0,
            Space::Md => 12.0,
            Space::Lg => 16.0,
            Space::Xl => 24.0,
            Space::Xxl => 32.0,
            Space::Huge => 48.0,
        }
    }

    /// Every step, for a design-system swatch.
    pub const ALL: [Space; 8] = [
        Space::None,
        Space::Xs,
        Space::Sm,
        Space::Md,
        Space::Lg,
        Space::Xl,
        Space::Xxl,
        Space::Huge,
    ];
}

/// A corner radius step.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Radius {
    None,
    Sm,
    Md,
    Lg,
    Xl,
    Pill,
}

impl Radius {
    /// The radius in logical pixels.
    pub const fn px(self) -> f32 {
        match self {
            Radius::None => 0.0,
            Radius::Sm => 4.0,
            Radius::Md => 8.0,
            Radius::Lg => 12.0,
            Radius::Xl => 16.0,
            Radius::Pill => 999.0,
        }
    }

    /// Every step, for a design-system swatch.
    pub const ALL: [Radius; 6] = [
        Radius::None,
        Radius::Sm,
        Radius::Md,
        Radius::Lg,
        Radius::Xl,
        Radius::Pill,
    ];
}

/// A drop shadow.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shadow {
    /// Vertical offset, in logical pixels.
    pub offset_y: f32,
    /// Blur radius, in logical pixels.
    pub blur: f32,
    /// How far the shadow spreads, as a fraction of the blur.
    pub spread: f32,
}

impl Shadow {
    pub const NONE: Shadow = Shadow {
        offset_y: 0.0,
        blur: 0.0,
        spread: 0.0,
    };
    /// A subtle shadow for cards.
    pub const SM: Shadow = Shadow {
        offset_y: 1.0,
        blur: 3.0,
        spread: 0.0,
    };
    /// The default shadow for raised surfaces.
    pub const MD: Shadow = Shadow {
        offset_y: 2.0,
        blur: 8.0,
        spread: 0.0,
    };
    /// A dramatic shadow for modal surfaces.
    pub const LG: Shadow = Shadow {
        offset_y: 8.0,
        blur: 24.0,
        spread: 0.0,
    };
}

/// A shadow step.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ShadowStep {
    None,
    Sm,
    Md,
    Lg,
}

impl ShadowStep {
    pub const fn shadow(self) -> Shadow {
        match self {
            ShadowStep::None => Shadow::NONE,
            ShadowStep::Sm => Shadow::SM,
            ShadowStep::Md => Shadow::MD,
            ShadowStep::Lg => Shadow::LG,
        }
    }
}

/// The role a piece of text plays.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextRole {
    /// The screen title.
    Display,
    /// A section heading.
    Title,
    /// A sub-heading.
    Subtitle,
    /// Running text.
    Body,
    /// A control label.
    Label,
    /// A hint or a caption.
    Caption,
    /// A large number (the score).
    Numeric,
}

impl TextRole {
    /// The font size, in logical pixels.
    pub const fn size(self) -> f32 {
        match self {
            TextRole::Display => 28.0,
            TextRole::Title => 20.0,
            TextRole::Subtitle => 16.0,
            TextRole::Body => 14.0,
            TextRole::Label => 12.0,
            TextRole::Caption => 11.0,
            TextRole::Numeric => 26.0,
        }
    }

    /// The line height, in logical pixels.
    pub const fn line_height(self) -> f32 {
        match self {
            TextRole::Display => 36.0,
            TextRole::Title => 28.0,
            TextRole::Subtitle => 24.0,
            TextRole::Body => 20.0,
            TextRole::Label => 16.0,
            TextRole::Caption => 16.0,
            TextRole::Numeric => 28.0,
        }
    }

    /// The relative font weight (400 = regular, 700 = bold).
    pub const fn weight(self) -> u16 {
        match self {
            TextRole::Display => 700,
            TextRole::Title => 600,
            TextRole::Subtitle => 600,
            TextRole::Body => 400,
            TextRole::Label => 500,
            TextRole::Caption => 400,
            TextRole::Numeric => 700,
        }
    }

    /// Whether the text should use **tabular figures**.
    ///
    /// Numbers that change (the score) must not make the layout jitter, so
    /// they use fixed-width digits.
    pub const fn tabular(self) -> bool {
        matches!(self, TextRole::Numeric)
    }

    /// The tracking (letter spacing), in logical pixels.
    pub const fn tracking(self) -> f32 {
        match self {
            TextRole::Display => -0.4,
            TextRole::Title => -0.2,
            TextRole::Numeric => 0.5,
            _ => 0.0,
        }
    }
}

/// The user's text-size preference, as a multiplier on the type scale.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextScale {
    Small,
    Normal,
    Large,
    ExtraLarge,
}

impl TextScale {
    /// The multiplier applied to every role size.
    pub const fn factor(self) -> f32 {
        match self {
            TextScale::Small => 0.875,
            TextScale::Normal => 1.0,
            TextScale::Large => 1.125,
            TextScale::ExtraLarge => 1.25,
        }
    }

    pub const ALL: [TextScale; 4] = [
        TextScale::Small,
        TextScale::Normal,
        TextScale::Large,
        TextScale::ExtraLarge,
    ];
}

/// The spacing, radius and shadow steps used by the UI.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DesignTokens {
    pub space: Space,
    pub radius: Radius,
    pub shadow: ShadowStep,
    /// The user's text size preference.
    pub text_scale: TextScale,
    /// The minimum tap target, in logical pixels.
    pub hit_target: f32,
}

impl Default for DesignTokens {
    fn default() -> Self {
        DesignTokens {
            space: Space::Md,
            radius: Radius::Md,
            shadow: ShadowStep::Sm,
            text_scale: TextScale::Normal,
            hit_target: 36.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_spacing_scale_is_on_the_grid() {
        for s in Space::ALL {
            let v = s.px();
            assert_eq!(v % 4.0, 0.0, "{s:?} is not a multiple of 4");
            assert!(v == 0.0 || (v % 8.0 == 0.0 || v == 4.0 || v == 12.0));
        }
    }

    #[test]
    fn the_radius_scale_is_on_the_grid() {
        for r in Radius::ALL {
            let v = r.px();
            assert!(v == 0.0 || v == 999.0 || v % 4.0 == 0.0);
        }
    }

    #[test]
    fn line_height_is_always_at_least_the_font_size() {
        for role in [
            TextRole::Display,
            TextRole::Title,
            TextRole::Subtitle,
            TextRole::Body,
            TextRole::Label,
            TextRole::Caption,
            TextRole::Numeric,
        ] {
            assert!(
                role.line_height() >= role.size(),
                "{role:?} has a line height below its font size"
            );
        }
    }

    #[test]
    fn only_numbers_use_tabular_figures() {
        assert!(TextRole::Numeric.tabular());
        assert!(!TextRole::Body.tabular());
        assert!(!TextRole::Display.tabular());
    }

    #[test]
    fn text_scales_are_monotonic() {
        let mut last = 0.0;
        for s in TextScale::ALL {
            assert!(s.factor() > last);
            last = s.factor();
        }
    }
}
