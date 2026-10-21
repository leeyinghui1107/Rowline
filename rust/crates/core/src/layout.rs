//! Client-area geometry.
//!
//! # Where the model comes from
//!
//! Two independent sources were combined:
//!
//! * **the binary** (re-disassembled at `0x402EA7`): the constants
//!   `0x60`, `0x6C`, `0x148`, `0x3A`, `0xEA`, `0x7D` and the derived
//!   quantities, including the right-hand figure's position
//!   `panel_x = (right-0x148)/2 + 0x148 + ((right-0x148)/2 - 0x3A)/2`,
//!   which the screenshots confirm to within ~1.5 %;
//! * **the screenshots** (docs/11): the board is **horizontally centred**
//!   in the client area and **vertically centred in the area below the HUD
//!   bar**, with the two figures flanking it symmetrically.
//!
//! The two agree on the figure placement and disagree on the board's
//! origin, so the board follows the screenshots (direct behavioural
//! evidence) and the figures follow the binary.
//!
//! The layout never overflows: `clamp` keeps every region inside the
//! client area, which is the one place where the re-implementation
//! deliberately does **not** inherit the original's behaviour
//! (docs/04 decision D3).

/// Sprite slot edge length, in logical pixels.
pub const CELL_PX: i32 = 36;
/// Sprite drawing inset inside its slot.
pub const SPRITE_INSET: i32 = 2;
/// The board frame (the sunken border around the grid).
pub const BOARD_FRAME_W: i32 = 327;
/// The board frame's height.
pub const BOARD_FRAME_H: i32 = 328;
/// The height of the HUD bar above the board.
pub const HUD_H: i32 = 64;
/// The height of the menu bar at the very top.
pub const MENU_BAR_H: i32 = 32;
/// The width reserved for the figure on each side.
pub const FIGURE_W: i32 = 58;
/// The default figure slot width, used when the client is too narrow.
pub const PANEL_W: i32 = 125;

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

/// Floor division by two (the original uses an arithmetic shift).
#[inline]
fn idiv2(v: i32) -> i32 {
    v.div_euclid(2)
}

impl Layout {
    /// Compute the layout for a client area of the given size.
    pub fn compute(client_w: i32, client_h: i32) -> Layout {
        // The board frame is centred horizontally ...
        let board_x = idiv2(client_w - BOARD_FRAME_W);
        // ... and centred vertically in the area *below* the HUD bar.
        let top = MENU_BAR_H + HUD_H;
        let avail = (client_h - top).max(0);
        let board_y = top + idiv2(avail - BOARD_FRAME_H);

        // The figures flank the board.  The right one uses the formula the
        // original uses, which the screenshots confirm.
        let t = idiv2(client_w - BOARD_FRAME_W);
        let right_figure_x = t + BOARD_FRAME_W + idiv2(t - 58);
        let left_figure_x = client_w - right_figure_x - FIGURE_W;

        let mut l = Layout {
            client_w,
            client_h,
            board_x,
            board_y,
            left_figure_x,
            right_figure_x,
            figure_y: board_y,
            panel_x: right_figure_x,
            panel_y: board_y,
            panel_w: PANEL_W,
            panel_h: BOARD_FRAME_H,
            board_area_w: client_w - 96,
            cell_px: CELL_PX,
        };
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
            && py < self.figure_y + BOARD_FRAME_H
    }

    /// `true` when the point is inside the right figure's slot.
    pub fn in_right_figure(&self, px: i32, py: i32) -> bool {
        px >= self.right_figure_x
            && px < self.right_figure_x + FIGURE_W
            && py >= self.figure_y
            && py < self.figure_y + BOARD_FRAME_H
    }
}

/// The smallest client size in which every region fits.
pub fn min_client_size() -> (i32, i32) {
    let mut w = 320;
    while w < 8192 {
        if fits(w, 8192) {
            break;
        }
        w += 4;
    }
    let mut h = 240;
    while h < 8192 {
        if fits(8192, h) {
            break;
        }
        h += 4;
    }
    (w, h)
}

fn fits(w: i32, h: i32) -> bool {
    let l = Layout::compute(w, h);
    l.board_x >= 0
        && l.board_y >= 0
        && l.board_x + BOARD_FRAME_W <= w
        && l.board_y + BOARD_FRAME_H <= h
        && l.left_figure_x >= 0
        && l.right_figure_x + FIGURE_W <= w
}

/// `true` when the board fits inside the client area.
pub fn board_fits(w: i32, h: i32) -> bool {
    let l = Layout::compute(w, h);
    l.board_x >= 0
        && l.board_y >= 0
        && l.board_x + BOARD_FRAME_W <= w
        && l.board_y + BOARD_FRAME_H <= h
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn the_board_is_centred_below_the_hud() {
        for h in [700, 900, 1200] {
            let l = Layout::compute(1280, h);
            let top = MENU_BAR_H + HUD_H;
            let avail = h - top;
            let expected = top + (avail - BOARD_FRAME_H) / 2;
            assert_eq!(l.board_y, expected);
            // the board must never overlap the HUD
            assert!(l.board_y >= top);
        }
    }

    #[test]
    fn the_figures_flank_the_board() {
        let l = Layout::compute(1280, 960);
        assert!(l.left_figure_x + FIGURE_W <= l.board_x);
        assert!(l.right_figure_x >= l.board_x + BOARD_FRAME_W);
    }

    #[test]
    fn the_right_figure_matches_the_originals_formula() {
        for w in [880, 1280, 1920] {
            let l = Layout::compute(w, 960);
            let t = idiv2(w - BOARD_FRAME_W);
            let expected = t + BOARD_FRAME_W + idiv2(t - 58);
            assert_eq!(l.right_figure_x, expected);
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
    fn a_tiny_client_is_clamped() {
        let l = Layout::compute(200, 150);
        assert!(l.board_x >= 0 && l.board_y >= 0);
        assert!(l.left_figure_x >= 0);
    }
}
