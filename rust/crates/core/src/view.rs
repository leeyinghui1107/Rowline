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
use crate::layout::{Layout, BOARD_FRAME_H, CELL_PX, SPRITE_INSET};
use crate::render::{Color, GradientDirection, Rect, RenderCmd, SpriteSheetId};
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

    // ---- the cells ------------------------------------------------------
    for r in 0..state.side() {
        for c in 0..state.side() {
            let value = state.colour_at(r, c);
            let (dx, dy) = l.cell_origin(r as i32, c as i32);
            let cell = Rect::new(dx as f32, dy as f32, CELL_PX as f32, CELL_PX as f32);
            // The plate, then its border.
            cmds.push(RenderCmd::RoundedRect {
                rect: cell,
                radius: style.cell_radius,
                color: style.cell_fill,
            });
            cmds.push(RenderCmd::Outline {
                rect: cell,
                color: style.cell_border,
            });
            if value == 0 {
                continue;
            }
            // The ball, inset inside the plate.
            cmds.push(RenderCmd::Sprite {
                sheet: SpriteSheetId::MAIN,
                src: ball_src(value, state.use_figures as u8, state.sheet_scale),
                dst: Point::new((dx + SPRITE_INSET) as f32, (dy + SPRITE_INSET) as f32),
                size: Size::new(
                    CELL_PX as f32 - 2.0 * SPRITE_INSET as f32,
                    CELL_PX as f32 - 2.0 * SPRITE_INSET as f32,
                ),
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
    fn every_cell_gets_a_plate_and_a_border() {
        let cmds = build_frame(&sample(), &ViewStyle::default());
        let plates = cmds
            .iter()
            .filter(|c| matches!(c, RenderCmd::RoundedRect { .. }))
            .count();
        // 1 backdrop + 81 plates
        assert_eq!(plates, 82);
        let borders = cmds
            .iter()
            .filter(|c| matches!(c, RenderCmd::Outline { .. }))
            .count();
        // 81 cell borders + 1 selection
        assert_eq!(borders, 82);
    }

    #[test]
    fn only_occupied_cells_draw_a_ball() {
        let cmds = build_frame(&sample(), &ViewStyle::default());
        let balls = cmds
            .iter()
            .filter(|c| matches!(c, RenderCmd::Sprite { .. }))
            .count();
        assert_eq!(balls, 2);
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
