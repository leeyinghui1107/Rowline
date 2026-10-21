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
/// The WCAG AA threshold for large text (>= 18.66 px bold / 24 px regular).
pub const AA_LARGE_TEXT: f32 = 3.0;
/// The WCAG threshold for non-text UI elements (WCAG 1.4.11).
pub const AA_NON_TEXT: f32 = 3.0;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palette::Palette;

    fn check(p: &Palette, fg: Rgba, bg: Rgba, min: f32, what: &str) {
        let r = contrast_ratio(fg, bg);
        assert!(
            r >= min,
            "{:?}: {what} contrast is {r:.2}, needs >= {min:.1}",
            p.kind,
        );
    }

    #[test]
    fn known_contrast_values() {
        // Black on white is the maximum.
        assert!((contrast_ratio(Rgba::BLACK, Rgba::WHITE) - 21.0).abs() < 0.01);
        // White on white is the minimum.
        assert!((contrast_ratio(Rgba::WHITE, Rgba::WHITE) - 1.0).abs() < 0.01);
    }

    #[test]
    fn dark_theme_text_meets_aa() {
        let p = Palette::DARK;
        for (bg, name) in [
            (p.background, "background"),
            (p.surface, "surface"),
            (p.surface_alt, "surface_alt"),
            (p.surface_raised, "surface_raised"),
            (p.surface_hover, "surface_hover"),
            (p.surface_active, "surface_active"),
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
    fn light_theme_text_meets_aa() {
        let p = Palette::LIGHT;
        for (bg, name) in [
            (p.background, "background"),
            (p.surface, "surface"),
            (p.surface_alt, "surface_alt"),
            (p.surface_raised, "surface_raised"),
            (p.surface_hover, "surface_hover"),
            (p.surface_active, "surface_active"),
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
        for p in [Palette::DARK, Palette::LIGHT] {
            check(&p, p.accent, p.background, AA_TEXT, "accent on background");
            check(&p, p.text_on_accent, p.accent, AA_TEXT, "text on accent");
        }
    }

    #[test]
    fn ball_colours_are_distinguishable_from_the_board() {
        for p in [Palette::DARK, Palette::LIGHT] {
            for (i, c) in p.balls.all().iter().enumerate() {
                let r = contrast_ratio(*c, p.surface);
                assert!(
                    r >= AA_NON_TEXT,
                    "{:?}: ball {} contrast {r:.2} on the board",
                    p.kind,
                    i + 1
                );
            }
        }
    }

    /// The minimum greyscale separation between two ball colours.
    ///
    /// A ratio of 1.10 means the two greys differ enough to be told apart.
    /// Seven fully distinct greys is a strong constraint; the **shape**
    /// channel (the "使用形状" sprite set) is the primary redundancy for
    /// colour-blind players, and luminance is the secondary one.
    pub const GREY_SEPARATION: f32 = 1.10;

    #[test]
    fn ball_colours_are_distinguishable_in_greyscale() {
        for p in [Palette::DARK, Palette::LIGHT] {
            let all = p.balls.all();
            for i in 0..all.len() {
                for j in (i + 1)..all.len() {
                    let r = contrast_ratio(all[i].to_grey(), all[j].to_grey());
                    assert!(
                        r >= GREY_SEPARATION,
                        "{:?}: balls {} and {} are too close in greyscale ({r:.2})",
                        p.kind,
                        i + 1,
                        j + 1
                    );
                }
            }
        }
    }

    #[test]
    fn ball_colours_are_all_distinct() {
        for p in [Palette::DARK, Palette::LIGHT] {
            let mut seen = Vec::new();
            for c in p.balls.all() {
                assert!(!seen.contains(&c), "{:?}: duplicate ball colour", p.kind);
                seen.push(c);
            }
        }
    }
}
