//! Bitmap asset resampling.
//!
//! Two strategies, in order of preference:
//!
//! 1. ship a **higher-resolution variant** of the asset (`@1x`, `@2x`, `@3x`)
//!    and pick the smallest one that still covers the physical pixels the
//!    asset will occupy -- the GPU then samples it 1:1 and it stays crisp;
//! 2. if no variant is large enough, **resample at runtime** with a
//!    Lanczos-3 kernel, which is noticeably better than bilinear for
//!    downscaling and avoids the hard pixel edges bilinear leaves behind.
//!
//! The resampler works on straight-alpha RGBA8 and is separable, so the
//! cost is `O(w * h)` rather than `O(w * h * k)`.

/// The Lanczos-3 kernel support radius.
pub const LANCZOS_A: f32 = 3.0;

/// Evaluate the Lanczos kernel.
#[inline]
fn lanczos(x: f32, a: f32) -> f32 {
    if x == 0.0 {
        return 1.0;
    }
    if x.abs() >= a {
        return 0.0;
    }
    let px = std::f32::consts::PI * x;
    (a * px.sin() * (px / a).sin()) / (px * px)
}

/// Build the resampling weights for one axis.
///
/// `src_len` is the source length, `dst_len` the destination length.
/// Returns `(first_index, weights)` per destination sample.
fn axis_weights(src_len: usize, dst_len: usize) -> Vec<(Vec<usize>, Vec<f32>)> {
    let mut result: Vec<(Vec<usize>, Vec<f32>)> = Vec::with_capacity(dst_len);
    if src_len == 0 || dst_len == 0 {
        return result;
    }
    let ratio = src_len as f32 / dst_len as f32;
    let support = LANCZOS_A * ratio.max(1.0);
    for i in 0..dst_len {
        // The centre of the destination pixel in source coordinates.
        let centre = (i as f32 + 0.5) * ratio - 0.5;
        let first = (centre - support).floor() as isize;
        let last = (centre + support).ceil() as isize;
        let mut idx = Vec::new();
        let mut w = Vec::new();
        let mut sum = 0.0f32;
        for s in first..=last {
            let weight = lanczos((s as f32 - centre) / ratio.max(1.0), LANCZOS_A);
            if weight != 0.0 {
                idx.push(s as usize);
                w.push(weight);
                sum += weight;
            }
        }
        if sum != 0.0 {
            for v in w.iter_mut() {
                *v /= sum;
            }
        }
        result.push((idx, w));
    }
    result
}

/// Resample an RGBA8 image with the Lanczos-3 kernel.
///
/// Returns `dst_w * dst_h * 4` bytes.
pub fn lanczos3_rgba(src: &[u8], src_w: u32, src_h: u32, dst_w: u32, dst_h: u32) -> Vec<u8> {
    let (sw, sh, dw, dh) = (
        src_w as usize,
        src_h as usize,
        dst_w as usize,
        dst_h as usize,
    );
    if sw == 0 || sh == 0 || dw == 0 || dh == 0 {
        return Vec::new();
    }
    if src.len() < sw * sh * 4 {
        return Vec::new();
    }
    let xw = axis_weights(sw, dw);
    let yw = axis_weights(sh, dh);

    // Horizontal pass: source (sw x sh) -> (dw x sh), float RGBA.
    let mut tmp = vec![0.0f32; dw * sh * 4];
    for (y, trow) in tmp.chunks_exact_mut(dw * 4).enumerate() {
        for (x, cell) in trow.chunks_exact_mut(4).enumerate() {
            let (idx, w) = &xw[x];
            let mut acc = [0.0f32; 4];
            for (k, &s) in idx.iter().enumerate() {
                let sx = s.min(sw - 1);
                let w = w[k];
                let o = (y * sw + sx) * 4;
                for (c, a) in acc.iter_mut().enumerate() {
                    *a += src[o + c] as f32 * w;
                }
            }
            cell.copy_from_slice(&acc);
        }
    }

    // Vertical pass: (dw x sh) -> (dw x dh).
    let mut out = vec![0u8; dw * dh * 4];
    for (y, orow) in out.chunks_exact_mut(dw * 4).enumerate() {
        let (idx, w) = &yw[y];
        for (x, cell) in orow.chunks_exact_mut(4).enumerate() {
            let mut acc = [0.0f32; 4];
            for (k, &s) in idx.iter().enumerate() {
                let sy = s.min(sh - 1);
                let w = w[k];
                let o = (sy * dw + x) * 4;
                for c in 0..4 {
                    acc[c] += tmp[o + c] * w;
                }
            }
            for c in 0..4 {
                cell[c] = acc[c].round().clamp(0.0, 255.0) as u8;
            }
        }
    }
    out
}

/// Resample to a target size, using Lanczos-3.
///
/// A no-op when the sizes already match.
pub fn resample_rgba(src: &[u8], src_w: u32, src_h: u32, dst_w: u32, dst_h: u32) -> Vec<u8> {
    if src_w == dst_w && src_h == dst_h {
        return src.to_vec();
    }
    lanczos3_rgba(src, src_w, src_h, dst_w, dst_h)
}

/// Pick the best asset variant for a display scale factor.
///
/// `available` are the integer scales that exist on disk (e.g. `[1, 2, 3]`),
/// `scale_factor` is the device pixel ratio the asset will be drawn at.
///
/// A variant `v` provides `logical_size * v` physical pixels for an asset
/// that is `logical_size` logical pixels wide, so it is sufficient when
/// `v >= scale_factor`.  The smallest sufficient variant wins; when even the
/// largest is too small the largest is returned as a best effort.
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
    fn identical_sizes_are_a_no_op() {
        let src = vec![10u8; 4 * 4 * 4];
        let out = resample_rgba(&src, 4, 4, 4, 4);
        assert_eq!(out, src);
    }

    #[test]
    fn lanczos_preserves_a_flat_colour() {
        let mut src = vec![0u8; 4 * 4 * 4];
        for p in src.chunks_exact_mut(4) {
            p.copy_from_slice(&[200, 100, 50, 255]);
        }
        let out = lanczos3_rgba(&src, 4, 4, 8, 8);
        assert_eq!(out.len(), 8 * 8 * 4);
        // The interior must keep the colour (the edges are filtered against
        // the zero border, so only the interior is checked).
        let o = (4 * 8 + 4) * 4;
        assert!((out[o] as i32 - 200).abs() <= 2);
        assert!((out[o + 1] as i32 - 100).abs() <= 2);
        assert!((out[o + 3] as i32 - 255).abs() <= 2);
    }

    #[test]
    fn lanczos_output_size_is_correct() {
        let src = vec![128u8; 16 * 16 * 4];
        assert_eq!(lanczos3_rgba(&src, 16, 16, 32, 24).len(), 32 * 24 * 4);
        assert_eq!(lanczos3_rgba(&src, 16, 16, 7, 3).len(), 7 * 3 * 4);
    }

    #[test]
    fn variant_selection_prefers_the_smallest_sufficient_one() {
        // 100% -> @1x, 125%/150% -> @2x, 200% -> @2x, 300% -> @3x.
        assert_eq!(select_variant(&[1, 2, 3], 1.0), Some(1));
        assert_eq!(select_variant(&[1, 2, 3], 1.25), Some(2));
        assert_eq!(select_variant(&[1, 2, 3], 1.5), Some(2));
        assert_eq!(select_variant(&[1, 2, 3], 2.0), Some(2));
        assert_eq!(select_variant(&[1, 2, 3], 3.0), Some(3));
        // Even the largest is too small: the largest is still the best effort.
        assert_eq!(select_variant(&[1, 2], 4.0), Some(2));
        assert_eq!(select_variant(&[], 2.0), None);
    }

    #[test]
    fn kernel_is_normalised() {
        let w = axis_weights(16, 8);
        for (_, weights) in w {
            let sum: f32 = weights.iter().sum();
            assert!((sum - 1.0).abs() < 1e-4, "sum was {sum}");
        }
    }
}
