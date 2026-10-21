//! A window backend that exists only in memory.
//!
//! Used by the head-less replay CLI, by CI and by the tests.  It lets the
//! game loop run unchanged without a display server, which is what makes
//! "play 300 scripted moves and diff the results" possible.

use rowline_core::input::{InputEvent, Size};

use crate::WindowBackend;

/// A window that draws nowhere and can be fed synthetic events.
#[derive(Debug, Clone)]
pub struct HeadlessWindow {
    size: Size,
    scale: f32,
    title: String,
    min_size: Size,
    quit: bool,
    pending: Vec<InputEvent>,
}

impl Default for HeadlessWindow {
    fn default() -> Self {
        HeadlessWindow::new(Size::new(1280.0, 960.0), 1.0)
    }
}

impl HeadlessWindow {
    /// A head-less window of the given size and scale factor.
    pub fn new(size: Size, scale: f32) -> Self {
        HeadlessWindow {
            size,
            scale,
            title: crate::PRODUCT_NAME.to_string(),
            min_size: Size::new(0.0, 0.0),
            quit: false,
            pending: Vec::new(),
        }
    }

    /// Queue an event for the next poll.
    pub fn push_event(&mut self, ev: InputEvent) {
        self.pending.push(ev);
    }

    /// The current title.
    pub fn title(&self) -> &str {
        &self.title
    }
}

impl WindowBackend for HeadlessWindow {
    fn poll_events(&mut self) -> Vec<InputEvent> {
        std::mem::take(&mut self.pending)
    }

    fn size(&self) -> Size {
        self.size
    }

    fn scale_factor(&self) -> f32 {
        self.scale
    }

    fn request_quit(&mut self) {
        self.quit = true;
    }

    fn set_min_size(&mut self, size: Size) {
        self.min_size = size;
    }

    fn set_title(&mut self, title: &str) {
        self.title = title.to_string();
    }
}

impl HeadlessWindow {
    /// `true` once a quit was requested.
    pub fn quit_requested(&self) -> bool {
        self.quit
    }

    /// The enforced minimum size.
    pub fn min_size(&self) -> Size {
        self.min_size
    }
}
