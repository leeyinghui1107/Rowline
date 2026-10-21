//! `wl-layout-check` -- the M1 layout acceptance gate, in Rust.
//!
//! Verifies, for a set of client sizes, that every region fits, that the
//! side panel does not overlap the board, and that the pixel <-> cell mapping
//! round-trips for all 81 cells.
//!
//! ```text
//! wl-layout-check
//! ```

use std::process::ExitCode;

use rowline_core::layout::{min_client_size, Layout, BOARD_FRAME_H, BOARD_FRAME_W, CELL_PX};

fn run(w: i32, h: i32, failures: &mut usize) {
    let mut l = Layout::compute(w, h);
    l.clamp();
    println!("\nclient {}x{}", w, h);
    println!(
        "  board origin = ({}, {})   frame = {}x{}",
        l.board_x, l.board_y, BOARD_FRAME_W, BOARD_FRAME_H
    );
    println!(
        "  panel origin = ({}, {})   size  = {}x{}",
        l.panel_x, l.panel_y, l.panel_w, l.panel_h
    );

    macro_rules! check {
        ($cond:expr, $($arg:tt)*) => {{
            let ok: bool = $cond;
            if !ok {
                *failures += 1;
            }
            println!("  {} {}", if ok { "OK  " } else { "FAIL" }, format!($($arg)*));
        }};
    }

    check!(
        l.board_x >= 0 && l.board_y >= 0,
        "board origin inside the client rect"
    );
    check!(
        l.board_x + BOARD_FRAME_W <= w,
        "board frame fits horizontally ({} + {} <= {})",
        l.board_x,
        BOARD_FRAME_W,
        w
    );
    check!(
        l.board_y + BOARD_FRAME_H <= h,
        "board frame fits vertically ({} + {} <= {})",
        l.board_y,
        BOARD_FRAME_H,
        h
    );
    check!(
        l.panel_x + l.panel_w <= w,
        "side panel fits horizontally ({} + {} <= {})",
        l.panel_x,
        l.panel_w,
        w
    );
    check!(
        l.panel_x >= l.board_x + BOARD_FRAME_W,
        "side panel does not overlap the board frame"
    );
    check!(l.cell_px == CELL_PX, "cell size is {} px", CELL_PX);

    let mut ok = true;
    for r in 0..9i32 {
        for c in 0..9i32 {
            let (px, py) = l.cell_origin(r, c);
            if px < 0 || py < 0 || px + l.cell_px > w || py + l.cell_px > h {
                ok = false;
                println!("       cell ({r},{c}) out of bounds at ({px},{py})");
                continue;
            }
            match l.hit_cell(px + l.cell_px / 2, py + l.cell_px / 2) {
                Some((hr, hc)) if hr == r && hc == c => {}
                _ => {
                    ok = false;
                    println!("       cell ({r},{c}) does not round-trip");
                }
            }
        }
    }
    check!(
        ok,
        "all 81 cells map to distinct in-bounds rects and round-trip"
    );

    check!(
        l.hit_cell(l.board_x - 1, l.board_y - 1).is_none(),
        "a point outside the board is rejected"
    );
}

fn main() -> ExitCode {
    let sizes: [(i32, i32); 6] = [
        (640, 480),
        (800, 600),
        (1024, 768),
        (1280, 800),
        (1440, 900),
        (1920, 1080),
    ];
    let (minw, minh) = min_client_size();
    println!("=== M1 layout acceptance (rust) ===");
    println!("minimum client size that fits every region: {minw}x{minh}");
    println!("(the original's formulas overflow below this -- defect D3 in docs/04)");

    let mut failures = 0usize;
    for (w, h) in sizes {
        println!(
            "\n--- {}x{} {} ---",
            w,
            h,
            if w >= minw && h >= minh {
                "(at/above minimum)"
            } else {
                "(BELOW minimum: clamped)"
            }
        );
        run(w, h, &mut failures);
    }

    println!(
        "\n{} ({} failure(s))",
        if failures == 0 { "PASSED" } else { "FAILED" },
        failures
    );
    if failures == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
