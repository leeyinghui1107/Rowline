//! Viewport computation: where the game is drawn inside the window.
//!
//! Two policies:
//!
//! | policy | behaviour | used by |
//! |---|---|---|
//! | [`ScalePolicy::Logical`] | the content fills the window; one logical unit = one physical pixel / `scale_factor` | Rowline (the board is a fixed 36 logical px) |
//! | [`ScalePolicy::Fixed`] | the content keeps a fixed logical resolution and is scaled to fit, preserving the aspect ratio | pixel-art games, and the "retro" look |
//!
//! With [`ScalePolicy::Fixed`] the content is centred and the remaining
//! space becomes **letterbox** (horizontal bars) or **pillarbox** (vertical
//! bars).  Integer scale factors are preferred because a non-integer
//! resample of pixel art produces shimmering and uneven pixel widths.

pub use rowline_core::input::Size;
pub use rowline_core::render::Rect;

/// How the content is placed inside the window.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ScalePolicy {
    /// The content fills the window; layout is recomputed for the client size.
    ///
    /// `scale_factor` converts logical units to physical pixels.
    Logical,
    /// The content keeps a fixed logical resolution and is scaled to fit.
    Fixed {
        /// The fixed logical width.
        width: u32,
        /// The fixed logical height.
        height: u32,
        /// Allow a fractional scale factor.
        ///
        /// When `false` (the default for pixel art) the scale is floored to an
        /// integer so that every source pixel maps to a whole number of
        /// destination pixels.
        allow_non_integer: bool,
    },
}

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
    /// For [`ScalePolicy::Logical`] this equals `scale_factor`.
    /// For [`ScalePolicy::Fixed`] it is the (possibly integer) fit factor.
    pub content_scale: f32,
    /// `true` when there are bars around the content.
    pub letterboxed: bool,
    /// `true` when `content_scale` is a whole number.
    pub integer_scale: bool,
}

impl Viewport {
    /// The scale to use for **font rasterisation**, in physical pixels per
    /// logical unit.
    ///
    /// For the fixed-resolution policy this is the fit factor rather than the
    /// display scale factor, so that a font stays crisp relative to the
    /// content it belongs to.
    pub fn text_scale(&self) -> f32 {
        self.content_scale
    }

    /// The scale to use for **bitmap assets**, in physical pixels per logical
    /// unit of the content.
    pub fn asset_scale(&self) -> f32 {
        self.content_scale
    }
}

/// Compute the viewport for a window.
///
/// `physical` is the client area in physical pixels, `scale_factor` the
/// display scale the window currently sits at.
pub fn compute_viewport(policy: &ScalePolicy, physical: Size, scale_factor: f32) -> Viewport {
    let sf = if scale_factor > 0.0 {
        scale_factor
    } else {
        1.0
    };
    let logical_size = Size::new(physical.width / sf, physical.height / sf);

    match *policy {
        ScalePolicy::Logical => Viewport {
            logical_size,
            physical_size: physical,
            scale_factor: sf,
            content_rect: Rect::new(0.0, 0.0, physical.width, physical.height),
            content_scale: sf,
            letterboxed: false,
            integer_scale: (sf - sf.round()).abs() < 1e-4,
        },
        ScalePolicy::Fixed {
            width,
            height,
            allow_non_integer,
        } => {
            let fw = width.max(1) as f32;
            let fh = height.max(1) as f32;
            let mut s = (physical.width / fw).min(physical.height / fh);
            if !s.is_finite() || s <= 0.0 {
                s = 1.0;
            }
            let mut integer = false;
            if !allow_non_integer {
                let f = s.floor();
                if f >= 1.0 {
                    s = f;
                    integer = true;
                }
            } else {
                integer = (s - s.round()).abs() < 1e-4;
            }
            let cw = fw * s;
            let ch = fh * s;
            let x = (physical.width - cw) * 0.5;
            let y = (physical.height - ch) * 0.5;
            Viewport {
                logical_size,
                physical_size: physical,
                scale_factor: sf,
                content_rect: Rect::new(x, y, cw, ch),
                content_scale: s,
                letterboxed: x > 0.5 || y > 0.5,
                integer_scale: integer,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f32, b: f32, eps: f32) -> bool {
        (a - b).abs() < eps
    }

    #[test]
    fn logical_policy_fills_the_window() {
        let v = compute_viewport(&ScalePolicy::Logical, Size::new(2560.0, 1440.0), 2.0);
        assert!(close(v.logical_size.width, 1280.0, 0.01));
        assert!(close(v.logical_size.height, 720.0, 0.01));
        assert_eq!(v.content_rect, Rect::new(0.0, 0.0, 2560.0, 1440.0));
        assert!(close(v.content_scale, 2.0, 0.001));
        assert!(!v.letterboxed);
    }

    #[test]
    fn fixed_policy_letterboxes_and_keeps_the_aspect_ratio() {
        // 640x480 content on a 2560x1440 screen:
        //   scale = min(2560/640, 1440/480) = min(4, 3) = 3
        //   content = 1920x1440, pillarboxed by 320 px on each side.
        let v = compute_viewport(
            &ScalePolicy::Fixed {
                width: 640,
                height: 480,
                allow_non_integer: false,
            },
            Size::new(2560.0, 1440.0),
            1.0,
        );
        assert!(close(v.content_scale, 3.0, 0.001));
        assert!(v.integer_scale);
        assert!(close(v.content_rect.width, 1920.0, 0.01));
        assert!(close(v.content_rect.height, 1440.0, 0.01));
        assert!(close(v.content_rect.x, 320.0, 0.01));
        assert!(close(v.content_rect.y, 0.0, 0.01));
        assert!(v.letterboxed);
    }

    #[test]
    fn fixed_policy_prefers_integer_scales() {
        // 1000 physical px across a 640-wide content: 1.5625 -> floored to 1.
        let v = compute_viewport(
            &ScalePolicy::Fixed {
                width: 640,
                height: 480,
                allow_non_integer: false,
            },
            Size::new(1000.0, 600.0),
            1.0,
        );
        assert!(close(v.content_scale, 1.0, 0.001));
        assert!(v.integer_scale);
    }

    #[test]
    fn fixed_policy_can_allow_fractional_scales() {
        let v = compute_viewport(
            &ScalePolicy::Fixed {
                width: 640,
                height: 480,
                allow_non_integer: true,
            },
            Size::new(1000.0, 600.0),
            1.0,
        );
        // min(1000/640, 600/480) = min(1.5625, 1.25) = 1.25
        assert!(close(v.content_scale, 1.25, 0.001));
        assert!(!v.integer_scale);
    }

    #[test]
    fn a_zero_scale_factor_does_not_divide_by_zero() {
        let v = compute_viewport(&ScalePolicy::Logical, Size::new(800.0, 600.0), 0.0);
        assert!(close(v.logical_size.width, 800.0, 0.01));
        assert!(close(v.content_scale, 1.0, 0.001));
    }

    #[test]
    fn fractional_dpi_is_handled() {
        // Windows 125% / 150% / 175%.
        for (dpr, expected) in [(1.25f32, 1024.0f32), (1.5, 853.333), (1.75, 731.428)] {
            let v = compute_viewport(&ScalePolicy::Logical, Size::new(1280.0, 720.0), dpr);
            assert!(close(v.logical_size.width, expected, 0.01));
            assert!(close(v.content_scale, dpr, 0.001));
        }
    }
}
