//! Spacing, radii and the type scale.
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

/// The role a piece of text plays.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextRole {
    /// A section heading.
    Title,
    /// Running text.
    Body,
    /// A control label.
    Label,
    /// A hint or a caption.
    Caption,
}

impl TextRole {
    /// The font size, in logical pixels.
    pub const fn size(self) -> f32 {
        match self {
            TextRole::Title => 20.0,
            TextRole::Body => 14.0,
            TextRole::Label => 12.0,
            TextRole::Caption => 11.0,
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
}
