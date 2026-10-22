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
///
/// The original's client area starts below the native menu bar; its HUD
/// occupies the first 44 logical pixels, because the board origin formula
/// puts the board at `y = 44` for the default 604x371 client.
pub const HUD_H: i32 = 44;
/// The height of the drawn menu bar, in logical pixels.
///
/// The original used a native Win32 menu, which lives outside the client
/// area.  Rowline draws the menu itself, so the window's client area is
/// `HUD_H + board` tall **plus** this strip; the game layout is computed for
/// the area below it (`Layout::compute_with_top`).
pub const MENU_BAR_H: i32 = 26;
/// Rows kept free at the bottom of the client for the raised window frame
/// (1 px gap + 2 px bevel + 1 px spare), so the board ends **above** the
/// frame instead of covering it.
pub const FRAME_RESERVE: i32 = 4;
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
    /// The height of the drawn menu strip above the game content.
    ///
    /// Zero for the pure layout formulas; the app uses [`MENU_BAR_H`] so the
    /// menu is not part of the original 604x371 play area.
    pub top: i32,
    /// The rows reserved at the bottom of the client for the window frame.
    pub bottom: i32,
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
    pub cell_px: i32,
}

impl Layout {
    /// The original's formulas, without clamping.
    ///
    /// The original lets the board overflow by one pixel at the default
    /// 604x371 client area; [`Layout::compute`] clamps that away, while this
    /// function keeps the raw values so the calibration can be tested.
    pub fn raw(client_w: i32, client_h: i32) -> Layout {
        Self::raw_framed(client_w, client_h, 0)
    }

    fn raw_framed(client_w: i32, client_h: i32, bottom: i32) -> Layout {
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
            top: 0,
            bottom,
            board_x,
            board_y,
            left_figure_x,
            right_figure_x,
            figure_y,
            panel_x: right_figure_x,
            panel_y: figure_y,
            panel_w: PANEL_W,
            panel_h: FIGURE_H,
            cell_px: CELL_PX,
        }
    }

    /// Compute the layout for a client area of the given size, clamped to it.
    pub fn compute(client_w: i32, client_h: i32) -> Layout {
        let mut l = Layout::raw(client_w, client_h);
        l.clamp();
        l
    }

    /// Compute the layout for content between a menu strip and the window's
    /// own bottom frame.
    ///
    /// `bottom` reserves rows at the bottom of the client for the raised
    /// window frame, so the board ends **above** the frame instead of
    /// covering it.  Everything else follows [`Layout::compute_with_top`].
    pub fn compute_framed(client_w: i32, client_h: i32, top: i32, bottom: i32) -> Layout {
        let top = top.clamp(0, client_h.max(0));
        let bottom = bottom.clamp(0, (client_h - top).max(0));
        let content_h = (client_h - top - bottom).max(1);
        let mut l = Layout::raw_framed(client_w, content_h, bottom);
        l.top = top;
        l.client_h = client_h;
        l.board_y += top;
        l.figure_y += top;
        l.panel_y += top;
        l.clamp();
        l
    }

    /// Keep every region inside the client area.
    pub fn clamp(&mut self) {
        if self.board_x < 0 {
            self.board_x = 0;
        }
        if self.board_y < self.top {
            self.board_y = self.top;
        }
        if self.board_x + BOARD_FRAME_W > self.client_w {
            self.board_x = (self.client_w - BOARD_FRAME_W).max(0);
        }
        // The board must end above the window's bottom frame.
        let limit = self.client_h - self.bottom;
        if self.board_y + BOARD_FRAME_H > limit {
            self.board_y = (limit - BOARD_FRAME_H).max(self.top);
        }
        if self.left_figure_x < 0 {
            self.left_figure_x = 0;
        }
        if self.right_figure_x + FIGURE_W > self.client_w {
            self.right_figure_x = (self.client_w - FIGURE_W).max(0);
        }
        if self.figure_y < self.top {
            self.figure_y = self.top;
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
    fn the_menu_strip_is_outside_the_play_area() {
        // The product adds the menu strip and the bottom frame reserve to the
        // original's 371-px play area; the content keeps the original
        // formulas (the 372-px content is the smallest one the board fits
        // into, so the board origin is still (138, 44) inside the content).
        let content_h = 372;
        let client_h = content_h + MENU_BAR_H + FRAME_RESERVE;
        let base = Layout::compute(604, content_h);
        let l = Layout::compute_framed(604, client_h, MENU_BAR_H, FRAME_RESERVE);
        assert_eq!(l.top, MENU_BAR_H);
        assert_eq!(l.bottom, FRAME_RESERVE);
        assert_eq!(l.client_h, client_h);
        assert_eq!(l.board_x, base.board_x);
        assert_eq!(l.board_y, base.board_y + MENU_BAR_H);
        assert_eq!(l.figure_y, base.figure_y + MENU_BAR_H);
        let (x, y) = l.cell_origin(0, 0);
        assert_eq!(l.hit_cell(x + 18, y + 18), Some((0, 0)));
    }

    #[test]
    fn the_board_ends_above_the_window_frame() {
        for (w, h) in [(604, 406), (800, 620), (1920, 1080)] {
            let l = Layout::compute_framed(w, h, MENU_BAR_H, FRAME_RESERVE);
            assert!(
                l.board_y + BOARD_FRAME_H <= h - FRAME_RESERVE,
                "{w}x{h}: the board covers the window frame (bottom {})",
                l.board_y + BOARD_FRAME_H
            );
            assert!(l.board_y >= MENU_BAR_H + HUD_H);
        }
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
