//! Viewport computation: where the game is drawn inside the window.
//!
//! The content always fills the window: one logical unit is one physical
//! pixel divided by the display's `scale_factor`, and the layout is
//! recomputed for the client size.  The board keeps its fixed 36 logical
//! pixels, so the game looks the same at every DPI while the window around
//! it grows or shrinks.

pub use rowline_core::input::Size;
pub use rowline_core::render::Rect;

/// The resolved placement of the content inside the window.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Viewport {
    /// The window's client area in **logical** units.
    pub logical_size: Size,
    /// The window's client area in **physical** pixels.
    pub physical_size: Size,
    /// The display scale factor the window currently sits at.
    pub scale_factor: f32,
    /// Where the content is drawn, in **physical** pixels.
    pub content_rect: Rect,
    /// Physical pixels per logical unit of the content.
    ///
    /// This equals the display's `scale_factor`.
    pub content_scale: f32,
    /// `true` when there are bars around the content.
    pub letterboxed: bool,
    /// `true` when `content_scale` is a whole number.
    pub integer_scale: bool,
}

/// Compute the viewport for a window.
///
/// `physical` is the client area in physical pixels, `scale_factor` the
/// display scale the window currently sits at.
pub fn compute_viewport(physical: Size, scale_factor: f32) -> Viewport {
    let sf = if scale_factor > 0.0 {
        scale_factor
    } else {
        1.0
    };
    let logical_size = Size::new(physical.width / sf, physical.height / sf);
    Viewport {
        logical_size,
        physical_size: physical,
        scale_factor: sf,
        content_rect: Rect::new(0.0, 0.0, physical.width, physical.height),
        content_scale: sf,
        letterboxed: false,
        integer_scale: (sf - sf.round()).abs() < 1e-4,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f32, b: f32, eps: f32) -> bool {
        (a - b).abs() < eps
    }

    #[test]
    fn the_content_fills_the_window() {
        let v = compute_viewport(Size::new(2560.0, 1440.0), 2.0);
        assert!(close(v.logical_size.width, 1280.0, 0.01));
        assert!(close(v.logical_size.height, 720.0, 0.01));
        assert_eq!(v.content_rect, Rect::new(0.0, 0.0, 2560.0, 1440.0));
        assert!(close(v.content_scale, 2.0, 0.001));
        assert!(!v.letterboxed);
    }

    #[test]
    fn a_zero_scale_factor_does_not_divide_by_zero() {
        let v = compute_viewport(Size::new(800.0, 600.0), 0.0);
        assert!(close(v.logical_size.width, 800.0, 0.01));
        assert!(close(v.content_scale, 1.0, 0.001));
    }

    #[test]
    fn fractional_dpi_is_handled() {
        // Windows 125% / 150% / 175%.
        for (dpr, expected) in [(1.25f32, 1024.0f32), (1.5, 853.333), (1.75, 731.428)] {
            let v = compute_viewport(Size::new(1280.0, 720.0), dpr);
            assert!(close(v.logical_size.width, expected, 0.01));
            assert!(close(v.content_scale, dpr, 0.001));
        }
    }
}
