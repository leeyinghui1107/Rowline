//! Colours and the two themes.
//!
//! Every colour is a **semantic token**, never a raw hex in the code: the UI
//! asks for `text_primary`, not for `#F2F4F8`.  That is what makes the
//! light/dark switch a one-line change and what keeps contrast verifiable.
//!
//! The palettes are chosen so that:
//!
//! * every text token reaches **WCAG AA (4.5:1)** against the surfaces it
//!   is used on -- this is asserted by the tests in `contrast.rs`, not by
//!   eyeballing;
//! * the seven ball colours stay distinguishable from each other and from
//!   the board surface (**3:1**, the non-text threshold);
//! * the hue relationships survive a greyscale conversion, because the game
//!   has a colour-blind friendly mode that turns the balls into shades.

use crate::contrast::{contrast_ratio, srgb_luminance};

/// An sRGB colour with straight alpha.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Rgba {
    pub const TRANSPARENT: Rgba = Rgba {
        r: 0,
        g: 0,
        b: 0,
        a: 0,
    };
    pub const BLACK: Rgba = Rgba {
        r: 0,
        g: 0,
        b: 0,
        a: 255,
    };
    pub const WHITE: Rgba = Rgba {
        r: 255,
        g: 255,
        b: 255,
        a: 255,
    };

    /// Build from a `0xRRGGBB` value, fully opaque.
    pub const fn hex(v: u32) -> Rgba {
        Rgba {
            r: ((v >> 16) & 0xFF) as u8,
            g: ((v >> 8) & 0xFF) as u8,
            b: (v & 0xFF) as u8,
            a: 255,
        }
    }

    /// Build from a `0xRRGGBBAA` value.
    pub const fn hexa(v: u32) -> Rgba {
        Rgba {
            r: ((v >> 24) & 0xFF) as u8,
            g: ((v >> 16) & 0xFF) as u8,
            b: ((v >> 8) & 0xFF) as u8,
            a: (v & 0xFF) as u8,
        }
    }

    /// The opaque colour.
    pub const fn opaque(self) -> Rgba {
        Rgba { a: 255, ..self }
    }

    /// The colour with a new alpha.
    pub const fn with_alpha(self, a: u8) -> Rgba {
        Rgba { a, ..self }
    }

    /// Linear interpolation in sRGB space (good enough for UI fills).
    pub fn lerp(self, other: Rgba, t: f32) -> Rgba {
        let t = t.clamp(0.0, 1.0);
        let m = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * t).round() as u8;
        Rgba {
            r: m(self.r, other.r),
            g: m(self.g, other.g),
            b: m(self.b, other.b),
            a: m(self.a, other.a),
        }
    }

    /// The greyscale equivalent, **preserving relative luminance**.
    ///
    /// The naive "luminance * 255" shortcut does not preserve luminance,
    /// because the sRGB transfer function is not linear; the inverse is
    /// used instead.
    pub fn to_grey(self) -> Rgba {
        let l = srgb_luminance(self);
        let v = if l <= 0.0031308 {
            12.92 * l
        } else {
            1.055 * l.powf(1.0 / 2.4) - 0.055
        };
        let v = (v * 255.0).round().clamp(0.0, 255.0) as u8;
        Rgba {
            r: v,
            g: v,
            b: v,
            a: self.a,
        }
    }

    /// The `0xRRGGBB` representation.
    pub const fn to_hex(self) -> u32 {
        ((self.r as u32) << 16) | ((self.g as u32) << 8) | (self.b as u32)
    }
}

impl From<Rgba> for rowline_core::render::Color {
    fn from(c: Rgba) -> rowline_core::render::Color {
        rowline_core::render::Color {
            r: c.r,
            g: c.g,
            b: c.b,
            a: c.a,
        }
    }
}

/// The colour of one of the seven ball types.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BallColors {
    pub c1: Rgba,
    pub c2: Rgba,
    pub c3: Rgba,
    pub c4: Rgba,
    pub c5: Rgba,
    pub c6: Rgba,
    pub c7: Rgba,
}

impl BallColors {
    /// The colour of ball `index` (1..=7).
    pub fn get(&self, index: u8) -> Rgba {
        match index {
            1 => self.c1,
            2 => self.c2,
            3 => self.c3,
            4 => self.c4,
            5 => self.c5,
            6 => self.c6,
            7 => self.c7,
            _ => Rgba::TRANSPARENT,
        }
    }

    /// All seven, in order.
    pub fn all(&self) -> [Rgba; 7] {
        [
            self.c1, self.c2, self.c3, self.c4, self.c5, self.c6, self.c7,
        ]
    }
}

/// Which theme is active.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThemeKind {
    Dark,
    Light,
}

impl ThemeKind {
    /// The other theme.
    pub fn toggled(self) -> ThemeKind {
        match self {
            ThemeKind::Dark => ThemeKind::Light,
            ThemeKind::Light => ThemeKind::Dark,
        }
    }
}

/// A complete set of semantic colour tokens.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Palette {
    pub kind: ThemeKind,

    // ---- surfaces ------------------------------------------------------
    /// The window background.
    pub background: Rgba,
    /// The board surface.
    pub surface: Rgba,
    /// A recessed surface (the side panel).
    pub surface_alt: Rgba,
    /// A raised surface (cards, buttons).
    pub surface_raised: Rgba,
    /// A hover state.
    pub surface_hover: Rgba,
    /// A pressed state.
    pub surface_active: Rgba,

    // ---- lines ---------------------------------------------------------
    pub outline: Rgba,
    pub outline_strong: Rgba,

    // ---- text ----------------------------------------------------------
    pub text_primary: Rgba,
    pub text_secondary: Rgba,
    pub text_tertiary: Rgba,
    pub text_on_accent: Rgba,

    // ---- accents -------------------------------------------------------
    pub accent: Rgba,
    pub accent_hover: Rgba,
    pub success: Rgba,
    pub warning: Rgba,
    pub danger: Rgba,

    // ---- effects -------------------------------------------------------
    pub shadow: Rgba,
    pub glow: Rgba,
    pub grid_line: Rgba,
    pub selection: Rgba,

    // ---- content -------------------------------------------------------
    pub balls: BallColors,
}

impl Palette {
    /// The dark theme, "Midnight".
    pub const DARK: Palette = Palette {
        kind: ThemeKind::Dark,
        background: Rgba::hex(0x12141A),
        surface: Rgba::hex(0x1B1E26),
        surface_alt: Rgba::hex(0x161920),
        surface_raised: Rgba::hex(0x242833),
        surface_hover: Rgba::hex(0x2C3140),
        surface_active: Rgba::hex(0x343A4B),
        outline: Rgba::hex(0x3A4050),
        outline_strong: Rgba::hex(0x565F73),
        text_primary: Rgba::hex(0xF2F4F8),
        text_secondary: Rgba::hex(0xB8BECE),
        text_tertiary: Rgba::hex(0x8E95A8),
        text_on_accent: Rgba::hex(0x12141A),
        accent: Rgba::hex(0xFFC24B),
        accent_hover: Rgba::hex(0xFFD066),
        success: Rgba::hex(0x5BD98A),
        warning: Rgba::hex(0xFFC24B),
        danger: Rgba::hex(0xFF7B7B),
        shadow: Rgba::hexa(0x000000A8),
        glow: Rgba::hexa(0xFFC24B40),
        grid_line: Rgba::hexa(0xFFFFFF0F),
        selection: Rgba::hex(0xFFC24B),
        // Chosen by measurement: every ball clears 3:1 against the board
        // surface, and the seven greyscale values stay >= 1.18 apart.
        balls: BallColors {
            c1: Rgba::hex(0x00DB16),
            c2: Rgba::hex(0x3870FF),
            c3: Rgba::hex(0x00A8B2),
            c4: Rgba::hex(0xFF75F1),
            c5: Rgba::hex(0xFF4242),
            c6: Rgba::hex(0xFFC42E),
            c7: Rgba::hex(0xC24600),
        },
    };

    /// The light theme, "Paper".
    pub const LIGHT: Palette = Palette {
        kind: ThemeKind::Light,
        background: Rgba::hex(0xF4F5F8),
        surface: Rgba::hex(0xFFFFFF),
        surface_alt: Rgba::hex(0xEDEFF3),
        surface_raised: Rgba::hex(0xFFFFFF),
        surface_hover: Rgba::hex(0xE6E9EF),
        surface_active: Rgba::hex(0xDCE0E8),
        outline: Rgba::hex(0xD2D6DF),
        outline_strong: Rgba::hex(0x9AA2B2),
        text_primary: Rgba::hex(0x14161C),
        text_secondary: Rgba::hex(0x454C5C),
        text_tertiary: Rgba::hex(0x656D7E),
        text_on_accent: Rgba::hex(0xFFFFFF),
        accent: Rgba::hex(0x9A5B00),
        accent_hover: Rgba::hex(0x7E4A00),
        success: Rgba::hex(0x186C3B),
        warning: Rgba::hex(0x8A5A00),
        danger: Rgba::hex(0xB3261E),
        shadow: Rgba::hexa(0x14161C24),
        glow: Rgba::hexa(0x9A5B0033),
        grid_line: Rgba::hexa(0x14161C12),
        selection: Rgba::hex(0x9A5B00),
        // Same constraints as the dark theme, inverted for a light board.
        balls: BallColors {
            c1: Rgba::hex(0x00850D),
            c2: Rgba::hex(0x002EA3),
            c3: Rgba::hex(0x00737A),
            c4: Rgba::hex(0x8F0081),
            c5: Rgba::hex(0xFF0505),
            c6: Rgba::hex(0xBD8800),
            c7: Rgba::hex(0xA83D00),
        },
    };

    /// The palette for a theme kind.
    pub fn of(kind: ThemeKind) -> Palette {
        match kind {
            ThemeKind::Dark => Palette::DARK,
            ThemeKind::Light => Palette::LIGHT,
        }
    }

    /// The text colour that meets the contrast requirement on `surface`.
    pub fn readable_on(&self, surface: Rgba) -> Rgba {
        if contrast_ratio(self.text_primary, surface) >= 4.5 {
            self.text_primary
        } else {
            self.text_secondary
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_round_trips() {
        assert_eq!(Rgba::hex(0xFFC24B).to_hex(), 0xFFC24B);
        assert_eq!(Rgba::hex(0xFFC24B).r, 0xFF);
        assert_eq!(Rgba::hex(0xFFC24B).b, 0x4B);
        assert_eq!(Rgba::hexa(0x11223344).a, 0x44);
    }

    #[test]
    fn lerp_endpoints() {
        let a = Rgba::hex(0x000000);
        let b = Rgba::hex(0xFFFFFF);
        assert_eq!(a.lerp(b, 0.0), a);
        assert_eq!(a.lerp(b, 1.0), b);
        let mid = a.lerp(b, 0.5);
        assert!((mid.r as i32 - 128).abs() <= 1);
    }

    #[test]
    fn greyscale_preserves_luminance() {
        let c = Rgba::hex(0xFFC24B);
        let g = c.to_grey();
        assert_eq!(g.r, g.g);
        assert_eq!(g.g, g.b);
        assert!((srgb_luminance(g) - srgb_luminance(c)).abs() < 0.01);
    }

    #[test]
    fn ball_lookup_is_stable() {
        let p = Palette::DARK;
        assert_eq!(p.balls.get(1), p.balls.c1);
        assert_eq!(p.balls.get(7), p.balls.c7);
        assert_eq!(p.balls.get(0), Rgba::TRANSPARENT);
        assert_eq!(p.balls.get(99), Rgba::TRANSPARENT);
    }
}
