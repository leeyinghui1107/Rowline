//! The software canvas: a CPU rasteriser for the command list.
//!
//! It is the single implementation of the software rendering rules, and it
//! backs two things:
//!
//! * the **softbuffer** backend, which presents the canvas in a window;
//! * the **offline screenshot tool** (`rowline-shot`), which renders a frame
//!   into a PNG without a GPU -- useful on build machines and in CI.
//!
//! Because both use the same code, a PNG produced offline is pixel-identical
//! to what the software backend shows on screen.

#[cfg(feature = "text")]
use std::collections::HashMap;

use image::RgbaImage;
use rowline_core::input::{Point, Size};
use rowline_core::render::{Color, FrameStyle, GradientDirection, Rect, RenderCmd};

/// A CPU rasteriser.
pub struct SoftwareCanvas {
    /// The pixels, as `0x00RRGGBB`.
    buffer: Vec<u32>,
    width: usize,
    height: usize,
    /// Where the content is drawn inside the target, in physical pixels.
    content_rect: Rect,
    /// Physical pixels per logical unit.
    content_scale: f32,
    /// The sprite sheet.
    sheet: Option<RgbaImage>,
    /// The icon atlas.
    icons: Option<rowline_assets::IconStrip>,
    /// The font file, kept so that atlases can be rasterised on demand.
    #[cfg(feature = "text")]
    font_bytes: Option<Vec<u8>>,
    /// One glyph atlas per nominal text size.
    #[cfg(feature = "text")]
    fonts: HashMap<u32, crate::font::FontAtlas>,
}

impl SoftwareCanvas {
    /// A canvas of the given size, in physical pixels.
    pub fn new(width: usize, height: usize) -> Self {
        SoftwareCanvas {
            buffer: vec![0; width * height],
            width,
            height,
            content_rect: Rect::new(0.0, 0.0, width as f32, height as f32),
            content_scale: 1.0,
            sheet: None,
            icons: None,
            #[cfg(feature = "text")]
            font_bytes: None,
            #[cfg(feature = "text")]
            fonts: HashMap::new(),
        }
    }

    /// Install the sprite sheet.
    pub fn set_sheet(&mut self, sheet: RgbaImage) {
        self.sheet = Some(sheet);
    }

    /// Install the icon atlas.
    pub fn set_icons(&mut self, strip: rowline_assets::IconStrip) {
        self.icons = Some(strip);
    }

    /// Install the font used for text.
    #[cfg(feature = "text")]
    pub fn set_font(&mut self, bytes: Vec<u8>) {
        self.font_bytes = Some(bytes);
        self.fonts.clear();
    }

    /// Resize the canvas; the content is cleared.
    pub fn begin(&mut self, size: Size, scale: f32) {
        let w = (size.width * scale).max(1.0) as usize;
        let h = (size.height * scale).max(1.0) as usize;
        if w != self.width || h != self.height {
            self.width = w;
            self.height = h;
            self.buffer = vec![0; w * h];
        }
    }

    /// Where the content is drawn inside the target, in physical pixels.
    pub fn set_viewport(&mut self, rect: Rect, content_scale: f32) {
        if (self.content_scale - content_scale).abs() > 1e-4 {
            // Glyph atlases bake the device pixel ratio into their
            // rasterisation, so a DPR change invalidates every cached size.
            self.fonts.clear();
        }
        self.content_rect = rect;
        self.content_scale = content_scale;
    }

    /// The canvas size, in physical pixels.
    pub fn size(&self) -> Size {
        Size::new(self.width as f32, self.height as f32)
    }

    /// Draw one command.
    pub fn submit(&mut self, cmd: RenderCmd) {
        match cmd {
            RenderCmd::Clear(c) => {
                let v = ((c.r as u32) << 16) | ((c.g as u32) << 8) | (c.b as u32);
                for px in self.buffer.iter_mut() {
                    *px = v;
                }
            }
            RenderCmd::Sprite { src, dst, size, .. } => {
                self.blit_sprite(src, dst.x as i32, dst.y as i32, size)
            }
            RenderCmd::Frame { rect, style } => self.frame(
                rect.x as i32,
                rect.y as i32,
                rect.width as i32,
                rect.height as i32,
                style,
            ),
            RenderCmd::Outline { rect, color } => {
                let k = self.content_scale;
                let x = (self.content_rect.x + rect.x * k) as i32;
                let y = (self.content_rect.y + rect.y * k) as i32;
                let w = (rect.width * k).max(1.0) as i32;
                let h = (rect.height * k).max(1.0) as i32;
                let t = k.max(1.0) as i32;
                self.fill_rect(x, y, w, t, color);
                self.fill_rect(x, y + h - t, w, t, color);
                self.fill_rect(x, y, t, h, color);
                self.fill_rect(x + w - t, y, t, h, color);
            }
            RenderCmd::RoundedRect {
                rect,
                radius,
                color,
            } => {
                self.rounded_rect(rect, radius, color);
            }
            RenderCmd::Gradient {
                rect,
                radius,
                from,
                to,
                direction,
            } => {
                self.gradient(rect, radius, from, to, direction);
            }
            RenderCmd::Shadow {
                rect,
                radius,
                offset,
                blur: _blur,
                color,
            } => {
                let r = Rect::new(
                    rect.x + offset.x,
                    rect.y + offset.y,
                    rect.width,
                    rect.height,
                );
                self.rounded_rect(r, radius, color);
            }
            RenderCmd::Icon {
                index,
                dst,
                size,
                tint,
            } => {
                self.draw_icon(index, dst, size, tint);
            }
            RenderCmd::Glow {
                rect,
                radius,
                blur: _blur,
                color,
            } => {
                self.rounded_rect(rect, radius, color);
            }
            RenderCmd::Text {
                text,
                at,
                size,
                color,
            } => {
                self.draw_text(&text, at, size, color);
            }
        }
    }

    /// The canvas as an image.
    pub fn to_image(&self) -> RgbaImage {
        let mut img = RgbaImage::new(self.width as u32, self.height as u32);
        for (i, px) in self.buffer.iter().enumerate() {
            let x = (i % self.width) as u32;
            let y = (i / self.width) as u32;
            img.put_pixel(
                x,
                y,
                image::Rgba([
                    ((px >> 16) & 0xFF) as u8,
                    ((px >> 8) & 0xFF) as u8,
                    (px & 0xFF) as u8,
                    255,
                ]),
            );
        }
        img
    }

    /// The raw pixels, as `0x00RRGGBB`.
    pub fn pixels(&self) -> &[u32] {
        &self.buffer
    }

    fn put(&mut self, x: i32, y: i32, c: Color) {
        if x < 0 || y < 0 || x as usize >= self.width || y as usize >= self.height {
            return;
        }
        let i = y as usize * self.width + x as usize;
        let src = ((c.r as u32) << 16) | ((c.g as u32) << 8) | (c.b as u32);
        let alpha = c.a as u32;
        if alpha == 255 {
            self.buffer[i] = src;
        } else if alpha > 0 {
            let dst = self.buffer[i];
            let rb = (src & 0x00FF_0000) >> 16;
            let gb = (src & 0x0000_FF00) >> 8;
            let bb = src & 0x0000_00FF;
            let dr = (dst >> 16) & 0xFF;
            let dg = (dst >> 8) & 0xFF;
            let db = dst & 0x0000_00FF;
            self.buffer[i] = (((rb * alpha + dr * (255 - alpha)) / 255) << 16)
                | (((gb * alpha + dg * (255 - alpha)) / 255) << 8)
                | ((bb * alpha + db * (255 - alpha)) / 255);
        }
    }

    fn fill_rect(&mut self, x: i32, y: i32, w: i32, h: i32, c: Color) {
        for yy in y..y + h {
            for xx in x..x + w {
                self.put(xx, yy, c);
            }
        }
    }

    fn frame(&mut self, x: i32, y: i32, w: i32, h: i32, style: FrameStyle) {
        // `top` is the top/left edge, `bottom` the bottom/right one.  Raised
        // catches the light on the top/left, sunken on the bottom/right --
        // the same mapping as the GPU backend.
        let (top, bottom) = match style {
            FrameStyle::Sunken => (Color::rgb(128, 128, 128), Color::rgb(255, 255, 255)),
            FrameStyle::Raised => (Color::rgb(255, 255, 255), Color::rgb(128, 128, 128)),
            FrameStyle::Flat => (Color::rgb(128, 128, 128), Color::rgb(128, 128, 128)),
        };
        // The rect arrives in logical units; scale it like every other
        // primitive.
        let k = self.content_scale;
        let x = (self.content_rect.x + x as f32 * k) as i32;
        let y = (self.content_rect.y + y as f32 * k) as i32;
        let w = (w as f32 * k).max(1.0) as i32;
        let h = (h as f32 * k).max(1.0) as i32;
        let t = k.max(1.0) as i32;
        self.fill_rect(x, y, w, t, top);
        self.fill_rect(x, y + h - t, w, t, bottom);
        self.fill_rect(x, y, t, h, top);
        self.fill_rect(x + w - t, y, t, h, bottom);
    }

    /// The icon atlas: a horizontal strip of square cells.
    fn draw_icon(&mut self, index: u32, dst: Point, size: Size, tint: Color) {
        // Copy the atlas out so that the buffer can be written at the same
        // time.
        let Some(strip) = self.icons.clone() else {
            return;
        };
        if strip.count == 0 {
            return;
        }
        let cell = strip.height;
        let sx = (index.min(strip.count - 1)) * cell;
        let k = self.content_scale;
        let dw = (size.width * k).max(1.0) as i32;
        let dh = (size.height * k).max(1.0) as i32;
        let ox = (self.content_rect.x + dst.x * k) as i32;
        let oy = (self.content_rect.y + dst.y * k) as i32;
        for j in 0..dh {
            for i in 0..dw {
                let tx = sx + (i as u32 * cell) / dw as u32;
                let ty = (j as u32 * cell) / dh as u32;
                let o = ((ty * strip.width + tx) * 4) as usize;
                if o + 3 >= strip.pixels.len() {
                    continue;
                }
                let a = strip.pixels[o + 3];
                if a == 0 {
                    continue;
                }
                // The atlas is white; the tint supplies the colour.
                let c = Color {
                    r: tint.r,
                    g: tint.g,
                    b: tint.b,
                    a: (tint.a as u32 * a as u32 / 255) as u8,
                };
                self.put(ox + i, oy + j, c);
            }
        }
    }

    /// Fill a rounded rectangle (software approximation of the SDF shape).
    fn rounded_rect(&mut self, rect: Rect, radius: f32, color: Color) {
        let k = self.content_scale;
        let x0 = (self.content_rect.x + rect.x * k) as i32;
        let y0 = (self.content_rect.y + rect.y * k) as i32;
        let w = (rect.width * k) as i32;
        let h = (rect.height * k) as i32;
        let r = (radius * k).max(0.0).min((w.min(h) as f32) * 0.5) as i32;
        for y in 0..h {
            for x in 0..w {
                // Distance to the rounded corner, if we are in one.
                let dx = if x < r {
                    r - x
                } else if x >= w - r {
                    x - (w - r - 1)
                } else {
                    0
                };
                let dy = if y < r {
                    r - y
                } else if y >= h - r {
                    y - (h - r - 1)
                } else {
                    0
                };
                if dx > 0 && dy > 0 && (dx * dx + dy * dy) > r * r {
                    continue;
                }
                self.put(x0 + x, y0 + y, color);
            }
        }
    }

    /// A linear gradient inside a rectangle.
    fn gradient(
        &mut self,
        rect: Rect,
        _radius: f32,
        from: Color,
        to: Color,
        direction: GradientDirection,
    ) {
        let k = self.content_scale;
        let x0 = (self.content_rect.x + rect.x * k) as i32;
        let y0 = (self.content_rect.y + rect.y * k) as i32;
        let w = (rect.width * k).max(1.0) as i32;
        let h = (rect.height * k).max(1.0) as i32;
        for y in 0..h {
            for x in 0..w {
                let t = match direction {
                    GradientDirection::Vertical => y as f32 / h as f32,
                    GradientDirection::Horizontal => x as f32 / w as f32,
                    GradientDirection::Radial => {
                        let cx = w as f32 * 0.5;
                        let cy = h as f32 * 0.5;
                        let d = ((x as f32 - cx).powi(2) + (y as f32 - cy).powi(2)).sqrt();
                        d / (w.max(h) as f32 * 0.5)
                    }
                };
                self.put(x0 + x, y0 + y, from.lerp(to, t.clamp(0.0, 1.0)));
            }
        }
    }

    fn blit_sprite(&mut self, src: Rect, dx: i32, dy: i32, size: Size) {
        let Some(sheet) = self.sheet.clone() else {
            return;
        };
        let sw = sheet.width() as i32;
        let sh = sheet.height() as i32;
        let sx = src.x as i32;
        let sy = src.y as i32;
        let cw = src.width as i32;
        let ch = src.height as i32;
        // The destination is in logical units; the canvas is physical, so
        // the sprite must grow with the content scale exactly like the
        // shapes and icons do.
        let k = self.content_scale;
        let dw = (size.width * k).max(1.0) as i32;
        let dh = (size.height * k).max(1.0) as i32;
        let ox = (self.content_rect.x + dx as f32 * k) as i32;
        let oy = (self.content_rect.y + dy as f32 * k) as i32;
        for j in 0..dh {
            for i in 0..dw {
                let tx = sx + (i * cw) / dw.max(1);
                let ty = sy + (j * ch) / dh.max(1);
                if tx < 0 || ty < 0 || tx >= sw || ty >= sh {
                    continue;
                }
                let p = *sheet.get_pixel(tx as u32, ty as u32);
                self.put(ox + i, oy + j, Color::rgba(p[0], p[1], p[2], p[3]));
            }
        }
    }

    /// Draw a run of text.
    ///
    /// Without the `text` feature -- or without a font -- a marker block is
    /// drawn instead, so that the presence of a label is still visible.
    #[cfg(feature = "text")]
    fn draw_text(&mut self, text: &str, at: Point, size: u16, color: Color) {
        let Some(bytes) = self.font_bytes.clone() else {
            self.draw_text_marker(text, at, size, color);
            return;
        };
        let dpr = self.content_scale;
        let key = size.max(1) as u32;
        if let std::collections::hash_map::Entry::Vacant(entry) = self.fonts.entry(key) {
            match crate::font::FontAtlas::new(bytes, size.max(1) as f32, dpr) {
                Ok(atlas) => {
                    entry.insert(atlas);
                }
                Err(e) => {
                    // Never panic on a diagnostics write: this renderer is
                    // also linked into the GUI-subsystem application.
                    {
                        use std::io::Write as _;
                        let _ =
                            writeln!(std::io::stderr(), "rowline: cannot rasterise the font: {e}");
                    }
                    self.draw_text_marker(text, at, size, color);
                    return;
                }
            }
        }
        let Some(atlas) = self.fonts.get_mut(&key) else {
            self.draw_text_marker(text, at, size, color);
            return;
        };
        let k = self.content_scale;
        // `at` is the top-left of the line, the glyph offsets are measured
        // from the baseline, so the ascent has to be added.
        let ascent = atlas.ascent();
        let mut quads = Vec::new();
        for (g, pen_x) in atlas.layout(text) {
            if g.is_empty() {
                continue;
            }
            let (ax, ay, gw, gh) = g.rect;
            quads.push((
                (self.content_rect.x + (at.x + pen_x + g.offset.0) * k) as i32,
                (self.content_rect.y + (at.y + g.offset.1 + ascent) * k) as i32,
                gw as i32,
                gh as i32,
                ax,
                ay,
            ));
        }
        let image = atlas.image().clone();
        for (dx, dy, gw, gh, ax, ay) in quads {
            for j in 0..gh {
                for i in 0..gw {
                    let tx = ax + i as u32;
                    let ty = ay + j as u32;
                    if tx >= image.width() || ty >= image.height() {
                        continue;
                    }
                    let p = image.get_pixel(tx, ty);
                    if p[3] == 0 {
                        continue;
                    }
                    let a = (color.a as u32 * p[3] as u32 / 255) as u8;
                    self.put(dx + i, dy + j, Color::rgba(color.r, color.g, color.b, a));
                }
            }
        }
    }

    #[cfg(feature = "text")]
    fn draw_text_marker(&mut self, text: &str, at: Point, size: u16, color: Color) {
        let w = (text.chars().count() as i32) * (size as i32) / 2;
        self.fill_rect(at.x as i32, at.y as i32, w.max(1), size as i32 / 2, color);
    }

    #[cfg(not(feature = "text"))]
    fn draw_text(&mut self, text: &str, at: Point, size: u16, color: Color) {
        let w = (text.chars().count() as i32) * (size as i32) / 2;
        self.fill_rect(at.x as i32, at.y as i32, w.max(1), size as i32 / 2, color);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_clear_fills_the_canvas() {
        let mut c = SoftwareCanvas::new(4, 3);
        c.submit(RenderCmd::Clear(Color::rgb(10, 20, 30)));
        assert!(c.pixels().iter().all(|&p| p == 0x000A_141E));
    }

    #[test]
    fn a_rectangle_is_drawn_at_the_right_place() {
        let mut c = SoftwareCanvas::new(8, 8);
        c.submit(RenderCmd::Clear(Color::BLACK));
        c.submit(RenderCmd::RoundedRect {
            rect: Rect::new(2.0, 2.0, 3.0, 3.0),
            radius: 0.0,
            color: Color::rgb(255, 0, 0),
        });
        assert_eq!(c.pixels()[2 * 8 + 2], 0x00FF_0000);
        assert_eq!(c.pixels()[4 * 8 + 4], 0x00FF_0000);
        assert_eq!(c.pixels()[0], 0);
    }

    #[test]
    fn the_viewport_moves_the_content() {
        let mut c = SoftwareCanvas::new(8, 8);
        c.set_viewport(Rect::new(4.0, 0.0, 4.0, 8.0), 1.0);
        c.submit(RenderCmd::Clear(Color::BLACK));
        c.submit(RenderCmd::RoundedRect {
            rect: Rect::new(0.0, 0.0, 2.0, 2.0),
            radius: 0.0,
            color: Color::rgb(0, 255, 0),
        });
        // The content starts at the viewport's origin, so it moved right.
        assert_eq!(c.pixels()[4], 0x0000_FF00);
        assert_eq!(c.pixels()[0], 0);
    }
}

#[cfg(all(test, feature = "text"))]
mod text_tests {
    use super::*;

    fn test_font() -> Vec<u8> {
        rowline_assets::embedded_font().to_vec()
    }

    #[test]
    fn text_is_drawn() {
        let mut c = SoftwareCanvas::new(200, 60);
        c.set_font(test_font());
        c.submit(RenderCmd::Clear(Color::BLACK));
        c.submit(RenderCmd::Text {
            text: "NEXT".to_string(),
            at: Point::new(10.0, 20.0),
            size: 20,
            color: Color::WHITE,
        });
        let lit = c.pixels().iter().filter(|&&p| p != 0).count();
        assert!(lit > 20, "only {lit} pixels were drawn");
    }
}
