//! The bundle the UI and the renderer consume.

use crate::palette::Palette;
use crate::tokens::{Radius, Space, TextRole};

/// Everything the UI needs, in one value.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DesignSystem {
    pub palette: Palette,
}

impl DesignSystem {
    /// The one design system the product ships: the original Classic look.
    pub fn new() -> Self {
        DesignSystem {
            palette: Palette::CLASSIC,
        }
    }

    /// The size of a text role.
    pub fn text_size(&self, role: TextRole) -> f32 {
        role.size()
    }

    /// The spacing step.
    pub fn space(&self, s: Space) -> f32 {
        s.px()
    }

    /// The corner radius.
    ///
    /// The product uses the original Win95 look: square controls.
    pub fn radius(&self, _r: Radius) -> f32 {
        0.0
    }
}

impl Default for DesignSystem {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_product_uses_the_classic_palette() {
        let ds = DesignSystem::new();
        assert_eq!(ds.palette, Palette::CLASSIC);
        assert_eq!(ds.radius(Radius::Lg), 0.0);
        assert!(ds.text_size(TextRole::Body) > 0.0);
    }
}
