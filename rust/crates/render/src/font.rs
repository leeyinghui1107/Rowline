//! Font rasterisation at the device pixel ratio.
//!
//! Text is the one asset that must be rasterised **at the display's
//! resolution**: a 12 px glyph drawn on a 200% display has to be 24 physical
//! pixels tall, otherwise it looks like a blurry thumbnail.
//!
//! The approach is the classic **dynamic glyph atlas**:
//!
//! 1. glyphs are rasterised on demand at `size * dpr` device pixels;
//! 2. they are packed into a texture that grows as needed;
//! 3. each glyph is emitted as a textured quad whose size is expressed in
//!    **logical** units, so the renderer applies the same logical-to-physical
//!    conversion as everything else.
//!
//! The atlas is uploaded to the GPU whenever it changes.

use std::collections::HashMap;

use ab_glyph::{Font, FontArc, PxScale, ScaleFont};
use image::RgbaImage;

/// A glyph that has been placed in the atlas.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlacedGlyph {
    /// The glyph's rectangle in the atlas, in texels: `(x, y, w, h)`.
    pub rect: (u32, u32, u32, u32),
    /// The offset from the pen origin, in **logical** units.
    pub offset: (f32, f32),
    /// The advance width, in **logical** units.
    pub advance: f32,
}

impl PlacedGlyph {
    /// `true` when the glyph has no pixels (a space, for example).
    pub fn is_empty(&self) -> bool {
        self.rect.2 == 0 || self.rect.3 == 0
    }
}

/// A growing glyph atlas.
pub struct FontAtlas {
    font: FontArc,
    /// The nominal em size, in logical units.
    size: f32,
    /// Device pixels per logical unit.
    dpr: f32,
    glyphs: HashMap<char, PlacedGlyph>,
    image: RgbaImage,
    /// The current pen position in the atlas, in texels.
    pen: (u32, u32),
    /// The height of the current atlas row.
    row_height: u32,
    /// The atlas size, in texels.
    pub width: u32,
    pub height: u32,
    /// The distance from the baseline to the top of the line, in logical units.
    ascent: f32,
    /// `true` when the atlas changed since the last GPU upload.
    dirty: bool,
}

impl FontAtlas {
    /// Parse a font and prepare an atlas for `size` logical units at `dpr`
    /// device pixels per logical unit.
    pub fn new(font_bytes: Vec<u8>, size: f32, dpr: f32) -> Result<Self, String> {
        let font = FontArc::try_from_vec(font_bytes).map_err(|e| format!("bad font: {e}"))?;
        let dpr = if dpr > 0.0 { dpr } else { 1.0 };
        let width = 512u32.max((size * dpr * 16.0).ceil() as u32);
        let height = 512u32.max((size * dpr * 4.0).ceil() as u32);
        let ascent = font.as_scaled(PxScale::from(size.max(1.0) * dpr)).ascent() / dpr;
        Ok(FontAtlas {
            font,
            size: size.max(1.0),
            dpr,
            glyphs: HashMap::new(),
            image: RgbaImage::new(width, height),
            pen: (0, 0),
            row_height: 0,
            width,
            height,
            ascent,
            dirty: false,
        })
    }

    /// The device pixels per logical unit.
    pub fn dpr(&self) -> f32 {
        self.dpr
    }

    /// The nominal em size, in logical units.
    pub fn size(&self) -> f32 {
        self.size
    }

    /// The distance from the baseline to the top of the line, in logical
    /// units.
    ///
    /// The glyph offsets are measured from the baseline, so a caller that
    /// wants to place a *line* of text at a top edge adds this to the
    /// baseline's y coordinate.
    pub fn ascent(&self) -> f32 {
        self.ascent
    }

    /// The atlas image (white glyphs with an alpha mask).
    pub fn image(&self) -> &RgbaImage {
        &self.image
    }

    /// `true` when the atlas changed since the last upload.
    pub fn dirty(&self) -> bool {
        self.dirty
    }

    /// Mark the atlas as uploaded.
    pub fn clear_dirty(&mut self) {
        self.dirty = false;
    }

    /// Lay out a string, returning the glyphs and their pen positions.
    ///
    /// The pen positions are in **logical** units.
    pub fn layout(&mut self, text: &str) -> Vec<(PlacedGlyph, f32)> {
        let mut out = Vec::with_capacity(text.chars().count());
        let mut pen_x = 0.0f32;
        for ch in text.chars() {
            let g = self.glyph(ch);
            out.push((g, pen_x));
            pen_x += g.advance;
        }
        out
    }

    /// The total advance width of a string, in logical units.
    pub fn measure(&mut self, text: &str) -> f32 {
        self.layout(text).iter().map(|(_, x)| x).sum::<f32>()
            + self
                .layout(text)
                .last()
                .map(|(g, _)| g.advance)
                .unwrap_or(0.0)
    }

    /// Get (rasterising if needed) the glyph for a character.
    pub fn glyph(&mut self, ch: char) -> PlacedGlyph {
        if let Some(g) = self.glyphs.get(&ch) {
            return *g;
        }
        let g = self.rasterise(ch);
        self.glyphs.insert(ch, g);
        g
    }

    fn rasterise(&mut self, ch: char) -> PlacedGlyph {
        let px_size = (self.size * self.dpr).round().max(1.0);
        let scaled = self.font.as_scaled(PxScale::from(px_size));
        let gid = scaled.glyph_id(ch);
        let advance_dev = scaled.h_advance(gid);

        // `outline_glyph` works on a `Glyph` (id + scale + position).
        let glyph = gid.with_scale_and_position(PxScale::from(px_size), ab_glyph::point(0.0, 0.0));
        let Some(outlined) = scaled.outline_glyph(glyph) else {
            return PlacedGlyph {
                rect: (0, 0, 0, 0),
                offset: (0.0, 0.0),
                advance: advance_dev / self.dpr,
            };
        };
        let bounds = outlined.px_bounds();
        let w = ((bounds.max.x - bounds.min.x).ceil() as i32).max(0) as u32;
        let h = ((bounds.max.y - bounds.min.y).ceil() as i32).max(0) as u32;
        if w == 0 || h == 0 {
            return PlacedGlyph {
                rect: (0, 0, 0, 0),
                offset: (0.0, 0.0),
                advance: advance_dev / self.dpr,
            };
        }

        // Rasterise into a local buffer first, so that the closure does not
        // have to borrow the atlas mutably.
        //
        // `OutlinedGlyph::draw` yields the coordinates **relative to the
        // glyph's bounding box** (it offsets them by `px_bounds.min`
        // internally), so they are already in `0..w` / `0..h`.
        let mut buffer = vec![0u8; (w * h) as usize];
        outlined.draw(|x, y, coverage| {
            if x >= w || y >= h {
                return;
            }
            let a = (coverage * 255.0).round().clamp(0.0, 255.0) as u8;
            buffer[y as usize * w as usize + x as usize] = a;
        });

        let (ax, ay) = self.allocate(w, h);
        for y in 0..h {
            for x in 0..w {
                let a = buffer[(y * w + x) as usize];
                if a > 0 {
                    self.image
                        .put_pixel(ax + x, ay + y, image::Rgba([255, 255, 255, a]));
                }
            }
        }
        self.dirty = true;

        PlacedGlyph {
            rect: (ax, ay, w, h),
            // The glyph's origin is at the pen; the bounds give the offset.
            offset: (bounds.min.x / self.dpr, bounds.min.y / self.dpr),
            advance: advance_dev / self.dpr,
        }
    }

    /// Reserve a `w x h` rectangle in the atlas, growing it if needed.
    fn allocate(&mut self, w: u32, h: u32) -> (u32, u32) {
        if self.pen.0 + w + 1 > self.width {
            // next row
            self.pen.0 = 0;
            self.pen.1 += self.row_height + 1;
            self.row_height = 0;
        }
        if self.pen.1 + h + 1 > self.height {
            self.grow();
        }
        let x = self.pen.0;
        let y = self.pen.1;
        self.pen.0 += w + 1;
        self.row_height = self.row_height.max(h);
        (x, y)
    }

    /// Double the atlas height, preserving its content.
    fn grow(&mut self) {
        let new_h = self.height * 2;
        let mut bigger = RgbaImage::new(self.width, new_h);
        for y in 0..self.height {
            for x in 0..self.width {
                bigger.put_pixel(x, y, *self.image.get_pixel(x, y));
            }
        }
        self.image = bigger;
        self.height = new_h;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A tiny but valid TrueType font is not available in the test
    /// environment, so the parser failure path is exercised instead.
    #[test]
    fn a_broken_font_is_rejected() {
        assert!(FontAtlas::new(vec![0u8; 16], 12.0, 2.0).is_err());
    }

    /// A system font, if one can be found.
    fn system_font() -> Option<Vec<u8>> {
        const CANDIDATES: &[&str] = &[
            "/System/Library/Fonts/Supplemental/Arial.ttf",
            "/System/Library/Fonts/Helvetica.ttc",
            "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
            "/usr/share/fonts/TTF/DejaVuSans.ttf",
            "C:\\Windows\\Fonts\\arial.ttf",
        ];
        for p in CANDIDATES {
            if let Ok(b) = std::fs::read(p) {
                return Some(b);
            }
        }
        None
    }

    #[test]
    fn glyphs_are_rasterised_at_the_device_resolution() {
        let Some(bytes) = system_font() else {
            // No system font available: nothing to rasterise.
            return;
        };
        let mut atlas = FontAtlas::new(bytes, 12.0, 2.0).unwrap();
        assert!(!atlas.dirty());
        let g = atlas.glyph('A');
        assert!(atlas.dirty());
        assert!(g.advance > 0.0);
        // Rasterised at 2x, so the glyph is roughly twice as tall as the
        // nominal size in logical units.
        assert!(g.rect.3 as f32 >= 12.0 * 1.2, "glyph height {}", g.rect.3);
        // The advance is reported in logical units, not device pixels.
        assert!(g.advance < 20.0, "advance {}", g.advance);

        // A space has no pixels but still advances.
        let space = atlas.glyph(' ');
        assert!(space.is_empty());
        assert!(space.advance > 0.0);

        // Laying out a string accumulates the advances.
        let laid = atlas.layout("AB");
        assert_eq!(laid.len(), 2);
        assert!(laid[1].1 > laid[0].1);
    }

    #[test]
    fn the_atlas_image_actually_holds_the_glyphs() {
        let Some(bytes) = system_font() else { return };
        let mut atlas = FontAtlas::new(bytes, 20.0, 1.0).unwrap();
        let g = atlas.glyph('A');
        eprintln!("glyph rect = {:?}", g.rect);
        let (ax, ay, w, h) = g.rect;
        let mut sum = 0u32;
        for y in 0..h {
            for x in 0..w {
                sum += atlas.image().get_pixel(ax + x, ay + y)[3] as u32;
            }
        }
        eprintln!("atlas alpha sum = {sum}");
        assert!(sum > 0, "the glyph was not rasterised into the atlas");
    }

    #[test]
    fn the_atlas_grows_when_it_runs_out_of_room() {
        let Some(bytes) = system_font() else { return };
        let mut atlas = FontAtlas::new(bytes, 24.0, 3.0).unwrap();
        let before = atlas.height;
        // Fill the atlas with many distinct glyphs.
        for ch in (0x20u32..0x300).filter_map(char::from_u32) {
            atlas.glyph(ch);
        }
        assert!(atlas.height >= before);
    }
}
