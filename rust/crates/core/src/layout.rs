//! Client-area geometry.
//!
//! # Where the model comes from
//!
//! The formulas are transcribed from the original's `WM_SIZE` handler.  The
//! handler appears twice in the binary (at `0x402EA7` and `0x4036E8`) with
//! identical arithmetic, so the constants are unambiguous:
//!
//! | global    | formula                             | meaning            |
//! |-----------|-------------------------------------|--------------------|
//! | `0x40B060`| `(clientW - 0x148) / 2`             | board x            |
//! | `0x40B064`| `(clientH - 0x178) / 2 + 0x2E`      | board y            |
//! | `0x40B070`| `clientW - 0x60`                    | board area width   |
//! | `0x40B078`| `(clientW - 0x6C) / 2`              | preview group x    |
//! | `0x40B088`| `((clientW - 0x148) / 2 - 0x3A) / 2`| left figure x      |
//! | `0x40B08C`| `(clientH - 0xEA) / 2 + 0xEA`       | figure base y      |
//! | `0x40B098`| `t + 0x148 + ((t - 0x3A) / 2)`, `t = (clientW - 0x148) / 2` | right figure x |
//!
//! `0x40B08C` is the bottom of the flanking figure slot; the portrait itself
//! starts `0x86` pixels higher (`0x4023B3`, `0x40254A`), so
//! `figure_y = (clientH - 0xEA) / 2 + 0xEA - 0x86`.
//!
//! A live client area of 604x371 gives the board origin `(138, 44)`, which
//! matches the black-box screenshots exactly.  The arithmetic is C integer
//! division (truncation toward zero), which Rust's `/` also does.
//!
//! The layout never overflows: `clamp` keeps every region inside the client
//! area, which is the one place where the re-implementation deliberately does
//! **not** inherit the original's behaviour (docs/04 decision D3).

/// Sprite slot edge length, in logical pixels.
pub const CELL_PX: i32 = 36;
/// Sprite drawing inset inside its slot.
pub const SPRITE_INSET: i32 = 2;
/// The board frame (the sunken border around the grid).
pub const BOARD_FRAME_W: i32 = 327;
/// The board frame's height.
pub const BOARD_FRAME_H: i32 = 328;
/// The width the original uses when centring the board (0x148).  The frame
/// itself is one pixel narrower, so the right margin is one pixel larger.
pub const BOARD_SPAN_W: i32 = 328;
/// The height of the HUD bar above the board.
pub const HUD_H: i32 = 64;
/// The height of the menu bar at the very top.
pub const MENU_BAR_H: i32 = 32;
/// The width reserved for the figure on each side.
pub const FIGURE_W: i32 = 58;
/// The height of the flanking figure slot (portrait plus pedestal).
pub const FIGURE_H: i32 = 134;
/// The default figure slot width, used when the client is too narrow.
pub const PANEL_W: i32 = 125;
/// The vertical bias the original subtracts before centring the board.
const BOARD_V_BIAS: i32 = 376;
/// The vertical offset the original adds after centring the board.
const BOARD_V_OFFSET: i32 = 46;
/// The vertical bias the original subtracts before placing the figures.
const FIGURE_V_BIAS: i32 = 234;
/// The vertical offset the original adds after centring the figures.
const FIGURE_V_OFFSET: i32 = 100;

/// The resolved geometry of the client area.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Layout {
    pub client_w: i32,
    pub client_h: i32,
    /// The top-left of the board frame.
    pub board_x: i32,
    pub board_y: i32,
    /// The left figure (the CPU, "国王").
    pub left_figure_x: i32,
    /// The right figure (the player, "挑战者").
    pub right_figure_x: i32,
    /// The top of the flanking figures' slots.
    pub figure_y: i32,
    /// The side panel (kept for the narrow-window fallback).
    pub panel_x: i32,
    pub panel_y: i32,
    pub panel_w: i32,
    pub panel_h: i32,
    /// The board area width, as the original computes it.
    pub board_area_w: i32,
    pub cell_px: i32,
}

impl Layout {
    /// The original's formulas, without clamping.
    ///
    /// The original lets the board overflow by one pixel at the default
    /// 604x371 client area; [`Layout::compute`] clamps that away, while this
    /// function keeps the raw values so the calibration can be tested.
    pub fn raw(client_w: i32, client_h: i32) -> Layout {
        // 0x40B060 / 0x40B064, exactly as the original's WM_SIZE does.
        let board_x = (client_w - BOARD_SPAN_W) / 2;
        let board_y = (client_h - BOARD_V_BIAS) / 2 + BOARD_V_OFFSET;

        // 0x40B088 / 0x40B098: the figures flank the board symmetrically,
        // both derived from the board's own x origin.
        let left_figure_x = (board_x - FIGURE_W) / 2;
        let right_figure_x = board_x + BOARD_SPAN_W + left_figure_x;

        // 0x40B08C minus the portrait's own offset (0x86).
        let figure_y = (client_h - FIGURE_V_BIAS) / 2 + FIGURE_V_OFFSET;

        Layout {
            client_w,
            client_h,
            board_x,
            board_y,
            left_figure_x,
            right_figure_x,
            figure_y,
            panel_x: right_figure_x,
            panel_y: figure_y,
            panel_w: PANEL_W,
            panel_h: FIGURE_H,
            board_area_w: client_w - 96,
            cell_px: CELL_PX,
        }
    }

    /// Compute the layout for a client area of the given size, clamped to it.
    pub fn compute(client_w: i32, client_h: i32) -> Layout {
        let mut l = Layout::raw(client_w, client_h);
        l.clamp();
        l
    }

    /// Keep every region inside the client area.
    pub fn clamp(&mut self) {
        if self.board_x < 0 {
            self.board_x = 0;
        }
        if self.board_y < 0 {
            self.board_y = 0;
        }
        if self.board_x + BOARD_FRAME_W > self.client_w {
            self.board_x = (self.client_w - BOARD_FRAME_W).max(0);
        }
        if self.board_y + BOARD_FRAME_H > self.client_h {
            self.board_y = (self.client_h - BOARD_FRAME_H).max(0);
        }
        if self.left_figure_x < 0 {
            self.left_figure_x = 0;
        }
        if self.right_figure_x + FIGURE_W > self.client_w {
            self.right_figure_x = (self.client_w - FIGURE_W).max(0);
        }
        if self.figure_y < 0 {
            self.figure_y = 0;
        }
    }

    /// The top-left of the grid, in logical pixels.
    pub fn cell_origin(&self, row: i32, col: i32) -> (i32, i32) {
        (
            self.board_x + SPRITE_INSET + col * self.cell_px,
            self.board_y + SPRITE_INSET + row * self.cell_px,
        )
    }

    /// Map a pixel back to a cell; `None` when the point is off the board.
    pub fn hit_cell(&self, px: i32, py: i32) -> Option<(i32, i32)> {
        let dx = px - (self.board_x + SPRITE_INSET);
        let dy = py - (self.board_y + SPRITE_INSET);
        if dx < 0 || dy < 0 {
            return None;
        }
        let r = dy / self.cell_px;
        let c = dx / self.cell_px;
        if r >= crate::board::BOARD_SIDE as i32 || c >= crate::board::BOARD_SIDE as i32 {
            return None;
        }
        Some((r, c))
    }

    /// `true` when the point is inside the left figure's slot.
    pub fn in_left_figure(&self, px: i32, py: i32) -> bool {
        px >= self.left_figure_x
            && px < self.left_figure_x + FIGURE_W
            && py >= self.figure_y
            && py < self.figure_y + FIGURE_H
    }

    /// `true` when the point is inside the right figure's slot.
    pub fn in_right_figure(&self, px: i32, py: i32) -> bool {
        px >= self.right_figure_x
            && px < self.right_figure_x + FIGURE_W
            && py >= self.figure_y
            && py < self.figure_y + FIGURE_H
    }
}

/// The smallest client size in which every region fits without clamping.
pub fn min_client_size() -> (i32, i32) {
    let mut w = 0;
    while w < 8192 && !fits_axis(w, true) {
        w += 1;
    }
    let mut h = 0;
    while h < 8192 && !fits_axis(h, false) {
        h += 1;
    }
    (w, h)
}

/// `true` when every region fits horizontally at `w` (or vertically at `h`),
/// using the *unclamped* original formulas for the tested axis.
fn fits_axis(v: i32, horizontal: bool) -> bool {
    let (w, h) = if horizontal { (v, 8192) } else { (8192, v) };
    let board_x = (w - BOARD_SPAN_W) / 2;
    let board_y = (h - BOARD_V_BIAS) / 2 + BOARD_V_OFFSET;
    let left_figure_x = (board_x - FIGURE_W) / 2;
    let right_figure_x = board_x + BOARD_SPAN_W + left_figure_x;
    let figure_y = (h - FIGURE_V_BIAS) / 2 + FIGURE_V_OFFSET;
    board_x >= 0
        && board_y >= 0
        && board_x + BOARD_FRAME_W <= w
        && board_y + BOARD_FRAME_H <= h
        && left_figure_x >= 0
        && left_figure_x + FIGURE_W <= board_x
        && right_figure_x >= board_x + BOARD_FRAME_W
        && right_figure_x + FIGURE_W <= w
        && figure_y >= 0
}

#[cfg(test)]
fn fits(w: i32, h: i32) -> bool {
    fits_axis(w, true) && fits_axis(h, false)
}

/// `true` when the board fits inside the (unclamped) client area.
pub fn board_fits(w: i32, h: i32) -> bool {
    let board_x = (w - BOARD_SPAN_W) / 2;
    let board_y = (h - BOARD_V_BIAS) / 2 + BOARD_V_OFFSET;
    board_x >= 0 && board_y >= 0 && board_x + BOARD_FRAME_W <= w && board_y + BOARD_FRAME_H <= h
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_original_604x371_client_is_reproduced() {
        // Direct black-box evidence: the board origin in the live original.
        let l = Layout::raw(604, 371);
        assert_eq!((l.board_x, l.board_y), (138, 44));
        assert_eq!(l.left_figure_x, 40);
        assert_eq!(l.right_figure_x, 506);
        assert_eq!(l.figure_y, 168);
    }

    #[test]
    fn the_board_matches_the_originals_formula() {
        for (w, h) in [
            (604, 371),
            (800, 600),
            (1024, 768),
            (1280, 960),
            (1920, 1080),
        ] {
            let l = Layout::raw(w, h);
            assert_eq!(l.board_x, (w - 328) / 2, "width {w}");
            assert_eq!(l.board_y, (h - 376) / 2 + 46, "height {h}");
        }
    }

    #[test]
    fn the_board_is_centred_horizontally() {
        for w in [880, 1280, 1920, 2560] {
            let l = Layout::compute(w, 960);
            let grid_mid = l.board_x + BOARD_FRAME_W / 2;
            assert!(
                (grid_mid - w / 2).abs() <= 2,
                "width {w}: the board is not centred ({grid_mid} vs {})",
                w / 2
            );
        }
    }

    #[test]
    fn the_figures_flank_the_board() {
        let l = Layout::compute(1280, 960);
        assert!(l.left_figure_x + FIGURE_W <= l.board_x);
        assert!(l.right_figure_x >= l.board_x + BOARD_FRAME_W);
        assert_eq!(l.right_figure_x + FIGURE_W + l.left_figure_x, l.client_w);
    }

    #[test]
    fn the_figures_match_the_originals_formula() {
        for w in [880, 1280, 1920] {
            let l = Layout::compute(w, 960);
            let t = (w - 328) / 2;
            assert_eq!(l.left_figure_x, (t - 58) / 2);
            assert_eq!(l.right_figure_x, t + 328 + (t - 58) / 2);
        }
    }

    #[test]
    fn the_figure_y_matches_the_originals_formula() {
        for h in [600, 768, 960] {
            let l = Layout::compute(1280, h);
            assert_eq!(l.figure_y, (h - 234) / 2 + 100);
        }
    }

    #[test]
    fn the_grid_round_trips() {
        let l = Layout::compute(1280, 960);
        for r in 0..9 {
            for c in 0..9 {
                let (px, py) = l.cell_origin(r, c);
                assert_eq!(l.hit_cell(px + 18, py + 18), Some((r, c)));
            }
        }
    }

    #[test]
    fn a_click_outside_the_board_misses() {
        let l = Layout::compute(1280, 960);
        assert_eq!(l.hit_cell(l.board_x - 1, l.board_y - 1), None);
    }

    #[test]
    fn the_minimum_client_size_is_the_smallest_fitting_one() {
        let (w, h) = min_client_size();
        assert!(fits(w, h));
        assert!(!fits(w - 1, h));
        assert!(!fits(w, h - 1));
        assert_eq!((w, h), (444, 372));
    }

    #[test]
    fn a_tiny_client_is_clamped() {
        let l = Layout::compute(200, 150);
        assert!(l.board_x >= 0 && l.board_y >= 0);
        assert!(l.left_figure_x >= 0);
        assert!(l.figure_y >= 0);
    }
}
