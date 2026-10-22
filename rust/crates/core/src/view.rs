//! Turning a [`GameState`] into a frame of render commands.
//!
//! The whole visual plan is a **pure function** of the state: no I/O, no
//! platform types.  That makes it unit-testable and guarantees that every
//! backend draws the same thing.
//!
//! The colours come in through [`ViewStyle`], so that `core` stays free of
//! any dependency on the design system (the `theme` crate depends on `core`,
//! not the other way round).

use crate::input::{Point, Size};
use crate::layout::{Layout, BOARD_FRAME_H, BOARD_FRAME_W, CELL_PX};
use crate::render::{Color, FrameStyle, GradientDirection, Rect, RenderCmd, SpriteSheetId};
use crate::state::GameState;

/// The colours and metrics the board is drawn with.
///
/// The application fills this in from the design system.
#[derive(Clone, Copy, Debug)]
pub struct ViewStyle {
    /// The window background, behind everything else.
    pub background: Color,
    /// The backdrop behind the board (the original uses black).
    pub board_backdrop: Color,
    /// The fill of an empty cell.
    pub cell_fill: Color,
    /// The border of a cell.
    pub cell_border: Color,
    /// The corner radius of a cell.
    pub cell_radius: f32,
    /// The colour of the selected cell.
    pub selection: Color,
    /// The colour of the path preview.
    pub path: Color,
    /// The highlight edge of the client frame (top / left).
    pub frame_light: Color,
    /// The shadow edge of the client frame (bottom / right).
    pub frame_dark: Color,
    /// How many pixels thick the client and board frames are; `0` disables
    /// them (the modern themes keep their flat look).
    pub frame_thickness: f32,
}

impl Default for ViewStyle {
    fn default() -> Self {
        ViewStyle {
            background: Color::BLACK,
            board_backdrop: Color::BLACK,
            cell_fill: Color::rgb(192, 192, 192),
            cell_border: Color::rgb(64, 64, 64),
            cell_radius: 4.0,
            selection: Color::rgb(255, 255, 0),
            path: Color::rgb(255, 255, 255),
            frame_light: Color::rgb(255, 255, 255),
            frame_dark: Color::rgb(128, 128, 128),
            frame_thickness: 0.0,
        }
    }
}

/// The classic Win95 button face used by the original.
pub const FACE: Color = Color::rgb(192, 192, 192);
/// The highlight colour used for the selected cell.
pub const HIGHLIGHT: Color = Color::rgb(255, 255, 0);
/// The text colour.
pub const TEXT: Color = Color::rgb(32, 32, 32);

/// Build the full command list for one frame.
pub fn build_frame(state: &GameState, style: &ViewStyle) -> Vec<RenderCmd> {
    let mut cmds = Vec::with_capacity(256);
    let l = state.layout;

    // ---- the window background ------------------------------------------
    cmds.push(RenderCmd::Clear(style.background));

    // ---- the black backdrop behind the board ---------------------------
    // The original paints the whole board strip black; the grid cells then
    // sit on top of it as light plates.
    cmds.push(RenderCmd::RoundedRect {
        rect: Rect::new(
            0.0,
            l.board_y as f32,
            l.client_w as f32,
            BOARD_FRAME_H as f32,
        ),
        radius: 0.0,
        color: style.board_backdrop,
    });

    // ---- the board's raised frame ---------------------------------------
    // The original's board sits in a 3D frame: light on the top/left, dark
    // on the bottom/right, with a one-pixel gap before the cell plates.
    if style.frame_thickness > 0.0 {
        cmds.push(RenderCmd::Frame {
            rect: Rect::new(
                (l.board_x - 2) as f32,
                l.board_y as f32,
                (BOARD_FRAME_W + 4) as f32,
                BOARD_FRAME_H as f32,
            ),
            style: FrameStyle::Raised,
        });
    }

    // ---- the cells ------------------------------------------------------
    for r in 0..state.side() {
        for c in 0..state.side() {
            let value = state.colour_at(r, c);
            let (dx, dy) = l.cell_origin(r as i32, c as i32);
            // The plate itself is a sprite: the original's empty-cell tile
            // carries the Win95 bevel (white top/left, dark bottom/right)
            // and the one-pixel black gap that forms the grid.
            cmds.push(RenderCmd::Sprite {
                sheet: SpriteSheetId::MAIN,
                src: ball_src(0, state.use_figures as u8, state.sheet_scale),
                dst: Point::new(dx as f32, dy as f32),
                size: Size::new(CELL_PX as f32, CELL_PX as f32),
                tint: Color::WHITE,
            });
            if value == 0 {
                continue;
            }
            // The ball.  It is sampled from the inner part of the cell,
            // inside the tile's own bevel, so it sits on the plate instead
            // of carrying a second frame.
            cmds.push(RenderCmd::Sprite {
                sheet: SpriteSheetId::MAIN,
                src: ball_inner_src(value, state.use_figures as u8, state.sheet_scale),
                dst: Point::new((dx + BALL_INSET) as f32, (dy + BALL_INSET) as f32),
                size: Size::new(BALL_PX as f32, BALL_PX as f32),
                tint: Color::WHITE,
            });
        }
    }

    // ---- selection and path --------------------------------------------
    if let Some(sel) = state.selected {
        let (r, c) = state.position(sel);
        let (dx, dy) = l.cell_origin(r as i32, c as i32);
        cmds.push(RenderCmd::Outline {
            rect: Rect::new(
                dx as f32 + 1.0,
                dy as f32 + 1.0,
                CELL_PX as f32 - 2.0,
                CELL_PX as f32 - 2.0,
            ),
            color: style.selection,
        });
    }
    for cell in &state.path {
        let (r, c) = state.position(*cell);
        let (dx, dy) = l.cell_origin(r as i32, c as i32);
        cmds.push(RenderCmd::Outline {
            rect: Rect::new(
                dx as f32 + 1.0,
                dy as f32 + 1.0,
                CELL_PX as f32 - 2.0,
                CELL_PX as f32 - 2.0,
            ),
            color: style.path,
        });
    }

    cmds
}

/// How far the ball sprite is inset inside its cell, in logical pixels.
///
/// A cell is 36 px: a 2 px bevel on the light edges, a 1 px shadow and the
/// 1 px black gap, which leaves a 30 px drawing area at (3, 3).
pub const BALL_INSET: i32 = 3;
/// The drawn diameter of a ball, in logical pixels.
pub const BALL_PX: i32 = 30;

/// The source rectangle of a ball **inside** the cell's bevel.
pub fn ball_inner_src(value: u8, sprite_set: u8, sheet_scale: u32) -> Rect {
    let mut src = ball_src(value, sprite_set, sheet_scale);
    let inset = BALL_INSET as f32 * sheet_scale.max(1) as f32;
    src.x += inset;
    src.y += inset;
    src.width = BALL_PX as f32 * sheet_scale.max(1) as f32;
    src.height = BALL_PX as f32 * sheet_scale.max(1) as f32;
    src
}

/// The source rectangle of a ball, following the index contract.
///
/// `sheet_scale` is the integer scale of the loaded sheet: 1 for the @1x
/// atlas, 2 for @2x, and so on.  The *destination* is always 36 logical
/// pixels; only the source rectangle grows, so the GPU samples the sheet
/// 1:1 on a high-DPI display.
pub fn ball_src(value: u8, sprite_set: u8, sheet_scale: u32) -> Rect {
    let s = sheet_scale.max(1) as f32;
    let col = (value as u32) + (sprite_set as u32) * 8;
    Rect::new((col as f32) * 36.0 * s, 225.0 * s, 36.0 * s, 36.0 * s)
}

/// The board's frame rectangle in logical pixels.
pub fn board_rect(l: &Layout) -> Rect {
    Rect::new(
        l.board_x as f32,
        l.board_y as f32,
        crate::layout::BOARD_FRAME_W as f32,
        crate::layout::BOARD_FRAME_H as f32,
    )
}

/// The gradient direction used for the accent bar of the HUD.
pub const ACCENT_DIRECTION: GradientDirection = GradientDirection::Horizontal;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board::Board;
    use crate::state::Phase;

    fn sample() -> GameState {
        let mut board = Board::new();
        board.set(0, 0, 3);
        board.set(4, 4, 7);
        GameState {
            phase: Phase::Ready,
            score: 120,
            best_score: 300,
            board,
            selected: Some(0),
            path: vec![],
            show_next: true,
            show_statistics: true,
            use_figures: false,
            next_group: [1, 2, 3],
            sheet_scale: 1,
            layout: Layout::compute(1280, 960),
        }
    }

    #[test]
    fn the_window_is_cleared_before_anything_is_drawn() {
        let cmds = build_frame(&sample(), &ViewStyle::default());
        assert!(
            matches!(cmds[0], RenderCmd::Clear(_)),
            "the first command should be the clear"
        );
    }

    #[test]
    fn the_backdrop_covers_the_board_strip() {
        let cmds = build_frame(&sample(), &ViewStyle::default());
        match cmds[1] {
            RenderCmd::RoundedRect { rect, color, .. } => {
                assert_eq!(rect.x, 0.0);
                assert_eq!(rect.width, 1280.0);
                assert_eq!(color, Color::BLACK);
            }
            _ => panic!("the second command should be the backdrop"),
        }
    }

    #[test]
    fn every_cell_draws_its_plate_from_the_atlas() {
        let cmds = build_frame(&sample(), &ViewStyle::default());
        let plates = cmds
            .iter()
            .filter(|c| matches!(c, RenderCmd::Sprite { size, .. } if size.width == CELL_PX as f32))
            .count();
        assert_eq!(plates, 81);
        // The only outline left is the selection ring; the bevel is baked
        // into the atlas tile.
        let borders = cmds
            .iter()
            .filter(|c| matches!(c, RenderCmd::Outline { .. }))
            .count();
        assert_eq!(borders, 1);
    }

    #[test]
    fn only_occupied_cells_draw_a_ball() {
        let cmds = build_frame(&sample(), &ViewStyle::default());
        let balls = cmds
            .iter()
            .filter(|c| matches!(c, RenderCmd::Sprite { size, .. } if size.width == BALL_PX as f32))
            .count();
        assert_eq!(balls, 2);
    }

    #[test]
    fn the_ball_is_sampled_inside_the_cell_bevel() {
        let src = ball_inner_src(5, 0, 2);
        assert_eq!(src.x, (5 * 36 + BALL_INSET) as f32 * 2.0);
        assert_eq!(src.y, (225 + BALL_INSET) as f32 * 2.0);
        assert_eq!(src.width, BALL_PX as f32 * 2.0);
        assert_eq!(src.height, BALL_PX as f32 * 2.0);
        // The sampled area stays inside the 36 px cell (a compile-time fact
        // for these constants).
        const _: () = assert!(BALL_INSET + BALL_PX <= CELL_PX);
    }

    #[test]
    fn ball_source_matches_the_contract() {
        assert_eq!(ball_src(0, 0, 1).x, 0.0);
        assert_eq!(ball_src(7, 0, 1).x, 252.0);
        assert_eq!(ball_src(7, 1, 1).x, 540.0);
        assert_eq!(ball_src(0, 0, 1).y, 225.0);
    }

    #[test]
    fn ball_source_scales_with_the_sheet() {
        assert_eq!(ball_src(7, 0, 2).x, 504.0);
        assert_eq!(ball_src(0, 0, 2).y, 450.0);
        assert_eq!(ball_src(3, 1, 3).width, 108.0);
    }
}
