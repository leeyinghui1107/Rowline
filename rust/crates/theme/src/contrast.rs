//! WCAG contrast utilities.
//!
//! These exist so that accessibility is **verified by the build**, not by
//! eyeballing a screenshot.

use crate::palette::Rgba;

/// The relative luminance of an sRGB colour (WCAG 2.1).
pub fn srgb_luminance(c: Rgba) -> f32 {
    fn lin(v: u8) -> f32 {
        let v = v as f32 / 255.0;
        if v <= 0.04045 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    }
    0.2126 * lin(c.r) + 0.7152 * lin(c.g) + 0.0722 * lin(c.b)
}

/// The WCAG contrast ratio between two colours, from 1.0 to 21.0.
pub fn contrast_ratio(a: Rgba, b: Rgba) -> f32 {
    let la = srgb_luminance(a);
    let lb = srgb_luminance(b);
    let (hi, lo) = if la > lb { (la, lb) } else { (lb, la) };
    (hi + 0.05) / (lo + 0.05)
}

/// The WCAG AA threshold for body text.
pub const AA_TEXT: f32 = 4.5;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palette::Palette;

    fn check(_p: &Palette, fg: Rgba, bg: Rgba, min: f32, what: &str) {
        let r = contrast_ratio(fg, bg);
        assert!(r >= min, "{what} contrast is {r:.2}, needs >= {min:.1}",);
    }

    #[test]
    fn known_contrast_values() {
        // Black on white is the maximum.
        assert!((contrast_ratio(Rgba::BLACK, Rgba::WHITE) - 21.0).abs() < 0.01);
        // White on white is the minimum.
        assert!((contrast_ratio(Rgba::WHITE, Rgba::WHITE) - 1.0).abs() < 0.01);
    }

    #[test]
    fn classic_text_meets_aa_on_controls() {
        let p = Palette::CLASSIC;
        for (bg, name) in [
            (p.surface_alt, "surface_alt"),
            (p.surface_raised, "surface_raised"),
        ] {
            check(
                &p,
                p.text_primary,
                bg,
                AA_TEXT,
                &format!("text_primary on {name}"),
            );
            check(
                &p,
                p.text_secondary,
                bg,
                AA_TEXT,
                &format!("text_secondary on {name}"),
            );
        }
    }

    #[test]
    fn accent_text_meets_aa_on_its_own_surface() {
        let p = Palette::CLASSIC;
        check(&p, p.text_on_accent, p.accent, AA_TEXT, "text on accent");
        check(
            &p,
            p.text_on_accent,
            p.surface_hover,
            AA_TEXT,
            "text on hover",
        );
    }

    #[test]
    fn ball_colours_are_all_distinct() {
        let p = Palette::CLASSIC;
        let mut seen = Vec::new();
        for c in p.balls.all() {
            assert!(!seen.contains(&c), "duplicate ball colour");
            seen.push(c);
        }
    }
}
