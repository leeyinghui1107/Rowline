//! Asset-variant selection.
//!
//! The sprite sheet is authored at several integer scales (`@1x` … `@4x`).
//! The renderer picks the smallest variant that covers the physical pixels
//! the asset occupies, so the GPU can sample it 1:1 on a high-DPI display.

/// The best embedded variant for a display scale factor.
///
/// Returns the smallest available scale at least as large as the display
/// scale, or the largest available scale when none is large enough.
pub fn select_variant(available: &[u32], scale_factor: f32) -> Option<u32> {
    let need = scale_factor.ceil().max(1.0) as u32;
    available
        .iter()
        .copied()
        .filter(|v| *v > 0)
        .filter(|v| *v >= need)
        .min()
        .or_else(|| available.iter().copied().filter(|v| *v > 0).max())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selects_the_smallest_covering_variant() {
        let scales = [1, 2, 3, 4];
        assert_eq!(select_variant(&scales, 1.0), Some(1));
        assert_eq!(select_variant(&scales, 1.5), Some(2));
        assert_eq!(select_variant(&scales, 2.0), Some(2));
        assert_eq!(select_variant(&scales, 2.5), Some(3));
    }

    #[test]
    fn falls_back_to_the_largest_variant() {
        assert_eq!(select_variant(&[1, 2], 3.0), Some(2));
        assert_eq!(select_variant(&[], 1.0), None);
    }
}
