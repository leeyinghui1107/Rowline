//! The palette and colour tokens.
//!
//! Every colour is a **semantic token**, never a raw hex in the code: the UI
//! asks for `text_primary`, not for `#000000`.  The product ships one
//! palette, the original WinLinez Classic look.
//!
//! UI text on the light controls is checked against **WCAG AA (4.5:1)** by
//! the tests in `contrast.rs`.  The seven ball colours are the measured
//! values from the original atlas; the board itself provides the contrast
//! through the beveled cell plates.

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

/// A complete set of semantic colour tokens.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Palette {
    // ---- surfaces ------------------------------------------------------
    /// The window background.
    pub background: Rgba,
    /// The strip behind the board (the original paints it black).
    pub board_backdrop: Rgba,
    /// The fill of an empty board cell (unused: the cell plate sprite is
    /// sampled from the atlas, but the token documents the colour).
    pub board_cell: Rgba,
    /// A recessed surface (the side panel).
    pub surface_alt: Rgba,
    /// A raised surface (cards, buttons).
    pub surface_raised: Rgba,
    /// A hover state.
    pub surface_hover: Rgba,

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
    pub danger: Rgba,

    // ---- effects -------------------------------------------------------
    pub shadow: Rgba,
    pub selection: Rgba,
    /// The dimming layer behind a modal dialog.
    pub scrim: Rgba,

    // ---- score readouts -------------------------------------------------
    /// The fill of the two HUD score boxes.
    pub score_fill: Rgba,
    /// The digits inside the score boxes.
    pub score_text: Rgba,
    /// The border around a score box.
    pub score_border: Rgba,

    // ---- content -------------------------------------------------------
    pub balls: BallColors,
}

impl Palette {
    /// The original WinLinez v1.30 look and the product's only theme.
    ///
    /// The original asks Windows for the 3D face colour (`COLOR_BTNFACE`),
    /// paints the board strip black and draws the cells from its atlas.  On
    /// current Windows that face colour is `#F0F0F0`; the cell plates in the
    /// original bitmap are `#C0C0C0`.  The seven ball colours are the ones
    /// recovered from the original atlas histogram.
    pub const CLASSIC: Palette = Palette {
        // The client area outside the HUD is the black board strip.
        background: Rgba::BLACK,
        board_backdrop: Rgba::BLACK,
        board_cell: Rgba::hex(0xC0C0C0),
        // Windows 10/11's `COLOR_BTNFACE`, used by the menu and HUD.
        surface_alt: Rgba::hex(0xF0F0F0),
        surface_raised: Rgba::hex(0xF0F0F0),
        // The Win95 menu highlight: navy with white text.
        surface_hover: Rgba::hex(0x000080),
        outline: Rgba::hex(0x808080),
        outline_strong: Rgba::BLACK,
        text_primary: Rgba::BLACK,
        text_secondary: Rgba::BLACK,
        // The original's `下组` / `颜色` labels use a mid grey; #606060 keeps
        // that look while clearing WCAG AA (5.2:1) on the #F0F0F0 bar.
        text_tertiary: Rgba::hex(0x606060),
        text_on_accent: Rgba::WHITE,
        accent: Rgba::hex(0x000080),
        danger: Rgba::hex(0x800000),
        // Win95 has no soft shadows or glows.
        shadow: Rgba::TRANSPARENT,
        selection: Rgba::hex(0x000080),
        scrim: Rgba::hexa(0x00000055),
        score_fill: Rgba::BLACK,
        score_text: Rgba::WHITE,
        score_border: Rgba::WHITE,
        balls: BallColors {
            c1: Rgba::hex(0x00A000),
            c2: Rgba::hex(0x0000C0),
            c3: Rgba::hex(0x00C0C0),
            c4: Rgba::hex(0xC000C0),
            c5: Rgba::hex(0xC00000),
            c6: Rgba::hex(0xC0C000),
            c7: Rgba::hex(0x800000),
        },
    };
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
    fn the_classic_palette_matches_the_original() {
        let p = Palette::CLASSIC;
        assert_eq!(p.background, Rgba::BLACK);
        assert_eq!(p.board_backdrop, Rgba::BLACK);
        assert_eq!(p.board_cell, Rgba::hex(0xC0C0C0));
        assert_eq!(p.surface_alt, Rgba::hex(0xF0F0F0));
        assert_eq!(p.score_fill, Rgba::BLACK);
        assert_eq!(p.score_text, Rgba::WHITE);
    }

    #[test]
    fn ball_lookup_is_stable() {
        let p = Palette::CLASSIC;
        assert_eq!(p.balls.get(1), p.balls.c1);
        assert_eq!(p.balls.get(7), p.balls.c7);
        assert_eq!(p.balls.get(0), Rgba::TRANSPARENT);
        assert_eq!(p.balls.get(99), Rgba::TRANSPARENT);
    }
}
