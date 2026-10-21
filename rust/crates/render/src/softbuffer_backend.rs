//! The software renderer.
//!
//! Blits sprites and draws frames with the CPU.  It needs no GPU driver,
//! which makes it the right choice for CI, for machines without a working
//! graphics stack, and as a **pixel-exact reference** for the GPU backend:
//! the differential replay gate can compare the two.
//!
//! The rasterisation itself lives in [`crate::canvas::SoftwareCanvas`], so
//! that the offline screenshot tool produces exactly the same pixels.

use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use softbuffer::{Context, Surface};

use rowline_core::input::Size;
use rowline_core::render::{Rect, RenderCmd};

use crate::Renderer;

/// A CPU renderer.
pub struct SoftbufferRenderer<'a, D: HasDisplayHandle, W: HasWindowHandle> {
    context: &'a Context<D>,
    surface: &'a mut Surface<D, W>,
    canvas: crate::canvas::SoftwareCanvas,
}

impl<'a, D: HasDisplayHandle, W: HasWindowHandle> SoftbufferRenderer<'a, D, W> {
    /// Bind a renderer to a surface.
    pub fn new(
        context: &'a Context<D>,
        surface: &'a mut Surface<D, W>,
        width: usize,
        height: usize,
    ) -> Self {
        SoftbufferRenderer {
            context,
            surface,
            canvas: crate::canvas::SoftwareCanvas::new(width, height),
        }
    }

    /// Install the sprite sheet.
    pub fn set_sheet(&mut self, sheet: image::RgbaImage) {
        self.canvas.set_sheet(sheet);
    }

    /// Install the icon atlas.
    pub fn set_icons(&mut self, strip: rowline_assets::IconStrip) {
        self.canvas.set_icons(strip);
    }

    /// Install the font used for text.
    #[cfg(feature = "text")]
    pub fn set_font(&mut self, bytes: Vec<u8>) {
        self.canvas.set_font(bytes);
    }
}

impl<D: HasDisplayHandle, W: HasWindowHandle> Renderer for SoftbufferRenderer<'_, D, W> {
    fn begin(&mut self, size: Size, scale: f32) {
        self.canvas.begin(size, scale);
    }

    fn submit(&mut self, cmd: RenderCmd) {
        self.canvas.submit(cmd);
    }

    fn end(&mut self) {
        // Copy the software buffer into the surface and present it.
        {
            let width = self.canvas.size().width as usize;
            let height = self.canvas.size().height as usize;
            let pixels = self.canvas.pixels();
            if let Ok(mut buffer) = self.surface.buffer_mut() {
                let bh = buffer.height().get() as usize;
                let bw = buffer.width().get() as usize;
                for y in 0..height.min(bh) {
                    let dst = &mut buffer[y * bw..(y + 1) * bw];
                    for x in 0..width.min(bw) {
                        dst[x] = pixels[y * width + x];
                    }
                }
            }
        }
        let _ = self.context;
        // `present` consumes the surface, which the caller owns; the caller
        // drives presentation in its own loop.
    }

    fn size(&self) -> Size {
        self.canvas.size()
    }

    fn set_viewport(&mut self, rect: Rect, content_scale: f32) {
        self.canvas.set_viewport(rect, content_scale);
    }
}
