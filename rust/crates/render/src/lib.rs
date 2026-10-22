//! Rendering abstraction.
//!
//! The game emits [`RenderCmd`] values; a backend turns them into pixels.
//! Two backends ship:
//!
//! | backend | feature | when to use |
//! |---|---|---|
//! | [`wgpu`] | `wgpu` | the product: GPU, HiDPI, every desktop platform + web |
//! | [`softbuffer`] | `softbuffer` | CI, machines without a GPU driver, pixel-exact reference |
//!
//! Both accept the same command stream, so the game logic cannot tell them
//! apart -- which is exactly what the differential replay gate relies on.

use rowline_core::input::Size;
use rowline_core::render::{Rect, RenderCmd};

/// A frame in progress.
pub trait Renderer {
    /// Start a frame of the given size (physical pixels) and scale factor.
    fn begin(&mut self, size: Size, scale: f32);
    /// Queue one command.
    fn submit(&mut self, cmd: RenderCmd);
    /// Finish and present the frame.
    fn end(&mut self);
    /// The size of the last frame.
    fn size(&self) -> Size;
    /// Where the content is drawn inside the target, and at what scale.
    ///
    /// `rect` is in **physical** pixels; with a letterboxed policy it is
    /// smaller than the target and the rest becomes bars.
    /// `content_scale` is the number of physical pixels per **logical** unit
    /// and is where the whole logical-to-physical conversion happens.
    fn set_viewport(&mut self, rect: Rect, content_scale: f32);
}

/// A renderer that records commands instead of drawing them.
///
/// Used by the head-less replay and by tests that need to assert on the
/// command stream.
#[derive(Debug, Default, Clone)]
pub struct RecordingRenderer {
    size: Size,
    scale: f32,
    content_rect: Rect,
    content_scale: f32,
    frames: Vec<Vec<RenderCmd>>,
    current: Vec<RenderCmd>,
}

impl RecordingRenderer {
    /// A fresh recorder.
    pub fn new() -> Self {
        RecordingRenderer::default()
    }

    /// The commands of the most recent frame.
    pub fn last_frame(&self) -> &[RenderCmd] {
        self.frames.last().map(|f| f.as_slice()).unwrap_or(&[])
    }

    /// The number of frames recorded.
    pub fn frame_count(&self) -> usize {
        self.frames.len()
    }
}

impl Renderer for RecordingRenderer {
    fn begin(&mut self, size: Size, scale: f32) {
        self.size = size;
        self.scale = scale;
        self.current.clear();
    }

    fn submit(&mut self, cmd: RenderCmd) {
        self.current.push(cmd);
    }

    fn end(&mut self) {
        self.frames.push(std::mem::take(&mut self.current));
    }

    fn size(&self) -> Size {
        self.size
    }

    fn set_viewport(&mut self, rect: Rect, content_scale: f32) {
        self.content_rect = rect;
        self.content_scale = content_scale;
    }
}

#[cfg(feature = "text")]
pub mod font;

/// The CPU rasteriser, shared by the software backend and the offline
/// screenshot tool.
#[cfg(feature = "image")]
pub mod canvas;

#[cfg(feature = "softbuffer")]
pub mod softbuffer_backend;

#[cfg(feature = "wgpu")]
pub mod wgpu_backend;

#[cfg(test)]
mod tests {
    use super::*;
    use rowline_core::render::Color;

    #[test]
    fn recorder_captures_frames() {
        let mut r = RecordingRenderer::new();
        r.begin(Size::new(100.0, 50.0), 1.0);
        r.submit(RenderCmd::Clear(Color::BLACK));
        r.end();
        assert_eq!(r.frame_count(), 1);
        assert_eq!(r.last_frame().len(), 1);
        assert_eq!(r.size(), Size::new(100.0, 50.0));
        r.set_viewport(Rect::new(10.0, 20.0, 80.0, 10.0), 2.0);
        assert_eq!(r.content_rect, Rect::new(10.0, 20.0, 80.0, 10.0));
        assert_eq!(r.content_scale, 2.0);
    }
}
