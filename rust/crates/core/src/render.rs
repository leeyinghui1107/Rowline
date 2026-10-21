//! The rendering command set.
//!
//! The game logic never touches a graphics API.  It emits [`RenderCmd`]
//! values, and the render layer decides how to turn them into GPU work.
//! That separation is what lets the same game run on wgpu, on a software
//! rasteriser, on the web, or be replayed into a text log in a test.

use crate::input::{Point, Size};

/// An RGBA colour with straight (non-premultiplied) alpha.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    /// Linear interpolation in sRGB space.
    pub fn lerp(self, other: Color, t: f32) -> Color {
        let t = t.clamp(0.0, 1.0);
        let m = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * t).round() as u8;
        Color {
            r: m(self.r, other.r),
            g: m(self.g, other.g),
            b: m(self.b, other.b),
            a: m(self.a, other.a),
        }
    }

    /// The colour with a new alpha.
    pub const fn with_alpha(self, a: u8) -> Color {
        Color { a, ..self }
    }

    pub const TRANSPARENT: Color = Color {
        r: 0,
        g: 0,
        b: 0,
        a: 0,
    };
    pub const BLACK: Color = Color {
        r: 0,
        g: 0,
        b: 0,
        a: 255,
    };
    pub const WHITE: Color = Color {
        r: 255,
        g: 255,
        b: 255,
        a: 255,
    };
    pub const GREY: Color = Color {
        r: 192,
        g: 192,
        b: 192,
        a: 255,
    };

    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Color { r, g, b, a: 255 }
    }

    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Color { r, g, b, a }
    }
}

/// An axis-aligned rectangle in logical pixels.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Rect {
    pub const fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Rect {
            x,
            y,
            width,
            height,
        }
    }

    pub fn contains(&self, p: Point) -> bool {
        p.x >= self.x && p.x < self.x + self.width && p.y >= self.y && p.y < self.y + self.height
    }
}

/// How a sunken / raised frame should look.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameStyle {
    /// Sunken into the surface (the original's board frame).
    Sunken,
    /// Raised out of the surface.
    Raised,
    /// Flat, single-pixel outline.
    Flat,
}

/// Identifies a sprite sheet.  The game only ever uses one today, but the
/// indirection keeps the render layer free of asset knowledge.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SpriteSheetId(pub u32);

impl SpriteSheetId {
    pub const MAIN: SpriteSheetId = SpriteSheetId(0);
}

/// The direction of a gradient.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GradientDirection {
    /// Top to bottom.
    Vertical,
    /// Left to right.
    Horizontal,
    /// From the centre outwards.
    Radial,
}

/// A command for the render layer.
#[derive(Clone, Debug, PartialEq)]
pub enum RenderCmd {
    /// Fill the whole target with a colour.
    Clear(Color),
    /// Draw a sub-rectangle of a sprite sheet, tinted.
    Sprite {
        sheet: SpriteSheetId,
        /// Source rectangle in texels.
        src: Rect,
        /// Destination top-left in logical pixels.
        dst: Point,
        /// Destination size in logical pixels.
        size: Size,
        /// Multiplicative tint.
        tint: Color,
    },
    /// Draw a 3D-ish frame.
    Frame { rect: Rect, style: FrameStyle },
    /// Draw a run of text.
    Text {
        text: String,
        at: Point,
        /// Nominal pixel height of the glyphs.
        size: u16,
        color: Color,
    },
    /// Draw a one-pixel outline.
    Outline { rect: Rect, color: Color },
    /// A filled rounded rectangle.
    RoundedRect {
        rect: Rect,
        radius: f32,
        color: Color,
    },
    /// A gradient inside a rounded rectangle.
    Gradient {
        rect: Rect,
        radius: f32,
        from: Color,
        to: Color,
        direction: GradientDirection,
    },
    /// A soft drop shadow cast by a rounded rectangle.
    Shadow {
        rect: Rect,
        radius: f32,
        offset: Point,
        blur: f32,
        color: Color,
    },
    /// An outer glow around a rounded rectangle.
    Glow {
        rect: Rect,
        radius: f32,
        blur: f32,
        color: Color,
    },
    /// An icon from the icon atlas, tinted.
    ///
    /// `index` is the position in the horizontal strip; the atlas is a row of
    /// square cells, so the source rectangle is derived from the index.
    Icon {
        index: u32,
        dst: Point,
        /// The size of the icon, in logical pixels.
        size: Size,
        tint: Color,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rect_containment_is_half_open() {
        let r = Rect::new(10.0, 10.0, 20.0, 20.0);
        assert!(r.contains(Point::new(10.0, 10.0)));
        assert!(r.contains(Point::new(29.9, 29.9)));
        assert!(!r.contains(Point::new(30.0, 30.0)));
        assert!(!r.contains(Point::new(9.9, 20.0)));
    }
}
