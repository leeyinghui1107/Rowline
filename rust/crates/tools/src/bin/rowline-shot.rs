//! `rowline-shot` -- render a frame of the game into a PNG, without a window.
//!
//! The tool drives the same code the application does -- the same game
//! state, the same layout and the same render commands -- and rasterises the
//! result with the software canvas, so the PNG is what the game would show.
//!
//! It exists because a build machine (or a CI runner) has no GPU and no
//! display: with this tool the visual result can be reviewed, and a
//! screenshot can be compared against a reference.
//!
//! ```text
//! rowline-shot --out shot.png --width 1280 --height 900 --theme dark
//! rowline-shot --out board.png --scenario board --scale 2
//! ```

use std::path::PathBuf;

use rowline_app::game::{frame, Game};
use rowline_core::layout::Layout;
use rowline_core::RuleModel;
use rowline_render::canvas::SoftwareCanvas;

/// The scenarios the tool can render.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Scenario {
    /// A fresh game.
    New,
    /// A game in progress, with a few balls on the board.
    Board,
    /// The leaderboard.
    Scoreboard,
    /// The about box.
    About,
    /// The end-of-game dialog.
    GameOver,
    /// A round at 160 against the 100-point record: the challenger stands
    /// above the king.
    Record,
    /// The moment after the record round: the king has taken the
    /// challenger's height and the challenger is back on the floor.
    AfterRecord,
    /// The name-entry dialog.
    NameEntry,
    /// A game in progress with the statistics window open.
    Stats,
    /// A just-completed five-ball clear, captured mid-dissolve.
    Clear,
}

fn main() {
    let mut out = PathBuf::from("rowline.png");
    let mut width = 1280.0_f32;
    let mut height = 900.0_f32;
    let mut scale = 1.0_f32;
    let mut theme = String::from("classic");
    let mut scenario = Scenario::New;
    let mut seed = 7_u32;

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        let mut value = || args.next().unwrap_or_default();
        match arg.as_str() {
            "--out" => out = PathBuf::from(value()),
            "--width" => width = value().parse().unwrap_or(width),
            "--height" => height = value().parse().unwrap_or(height),
            "--scale" => scale = value().parse().unwrap_or(scale),
            "--seed" => seed = value().parse().unwrap_or(seed),
            "--theme" => theme = value(),
            "--scenario" => {
                scenario = match value().as_str() {
                    "board" => Scenario::Board,
                    "scoreboard" => Scenario::Scoreboard,
                    "about" => Scenario::About,
                    "gameover" => Scenario::GameOver,
                    "record" => Scenario::Record,
                    "afterrecord" => Scenario::AfterRecord,
                    "nameentry" => Scenario::NameEntry,
                    "stats" => Scenario::Stats,
                    "clear" => Scenario::Clear,
                    _ => Scenario::New,
                }
            }
            "--help" | "-h" => {
                eprintln!(
                    "usage: rowline-shot [--out FILE] [--width N] [--height N] \
                     [--scale N] [--theme classic|dark|light] [--scenario new|board|\
                     scoreboard|about|gameover|nameentry|stats|clear] [--seed N]"
                );
                return;
            }
            other => {
                eprintln!("rowline-shot: unknown argument {other}");
                std::process::exit(2);
            }
        }
    }

    let mut game = Game::new(seed, RuleModel::default());
    match theme.as_str() {
        "dark" => game.set_theme(rowline_theme::ThemeKind::Dark),
        "light" => game.set_theme(rowline_theme::ThemeKind::Light),
        // Classic is the product default.
        _ => {}
    }
    let sheet = load_sheet(scale);
    if let Some((_image, sheet_scale)) = &sheet {
        game.set_sheet_scale(*sheet_scale);
    }

    let layout = Layout::compute_framed(
        width as i32,
        height as i32,
        rowline_core::layout::MENU_BAR_H,
        rowline_core::layout::FRAME_RESERVE,
    );
    match scenario {
        Scenario::New => {}
        Scenario::Board => {
            // A handful of balls, so that the board is not empty.
            game.engine_mut().new_game();
        }
        Scenario::Scoreboard => {
            game.run_command(rowline_app::commands::Command::SHOW_HIGH_SCORES);
        }
        Scenario::About => {
            game.run_command(rowline_app::commands::Command::ABOUT);
        }
        Scenario::GameOver => {
            game.engine_mut().new_game();
            game.set_game_over(true);
        }
        Scenario::Record => {
            // A round at 160 against the default record of 100: the
            // challenger stands above the king and has reached the cap.
            game.engine_mut().set_score(160);
            game.finish_figure_animation();
        }
        Scenario::AfterRecord => {
            game.engine_mut().set_score(160);
            game.finish_figure_animation();
            game.start_new_game();
            // The still frame shows both animations finished.
            game.finish_figure_animation();
        }
        Scenario::NameEntry => {
            game.engine_mut().set_score(42);
            game.offer_name_entry();
        }
        Scenario::Stats => {
            game.engine_mut().new_game();
            game.run_command(rowline_app::commands::Command::TOGGLE_STATISTICS);
        }
        Scenario::Clear => {
            // Four in a row plus a source ball one cell below the gap: the
            // move completes five and starts the dissolve.
            {
                let b = game.engine_mut().board_mut();
                b.cells_mut().fill(0);
                for c in 0..4 {
                    b.set(4, c, 1);
                }
                b.set(5, 4, 1);
            }
            let cell = |r: i32, c: i32| {
                let (x, y) = layout.cell_origin(r, c);
                rowline_core::input::Point::new(x as f32 + 18.0, y as f32 + 18.0)
            };
            // The offline shot wants the dissolve, not a frozen mid-move:
            // apply the engine move immediately, then restore animations.
            game.toggle_reduced_motion();
            game.click_at(cell(5, 4), layout);
            game.click_at(cell(4, 4), layout);
            game.toggle_reduced_motion();
        }
    }

    let cmds = frame(&game, layout);

    // ---- rasterise ------------------------------------------------------
    let mut canvas = SoftwareCanvas::new((width * scale) as usize, (height * scale) as usize);
    canvas.set_viewport(
        rowline_core::render::Rect::new(0.0, 0.0, width * scale, height * scale),
        scale,
    );

    if let Some((image, _scale)) = sheet {
        canvas.set_sheet(image);
    }
    if let Some(strip) = load_icons() {
        canvas.set_icons(strip);
    }
    if let Some(bytes) = load_font() {
        canvas.set_font(bytes);
    }

    canvas.begin(rowline_core::input::Size::new(width, height), scale);
    for cmd in cmds {
        canvas.submit(cmd);
    }

    let img = canvas.to_image();
    if let Some(parent) = out.parent() {
        if !parent.as_os_str().is_empty() {
            let _ = std::fs::create_dir_all(parent);
        }
    }
    match img.save(&out) {
        Ok(()) => println!("wrote {} ({}x{})", out.display(), img.width(), img.height()),
        Err(e) => {
            eprintln!("rowline-shot: cannot write {}: {e}", out.display());
            std::process::exit(1);
        }
    }
}

/// The sprite sheet, at the best variant for the scale.
fn load_sheet(scale: f32) -> Option<(image::RgbaImage, u32)> {
    // The same embedded atlas the application uses: the shot tool works
    // from any directory and needs no assets folder.
    let variant =
        rowline_display::select_variant(&rowline_assets::EMBEDDED_SHEET_SCALES, scale).unwrap_or(1);
    match rowline_assets::embedded_sheet_image(variant) {
        Ok(sheet) => Some((sheet.image().clone(), sheet.scale())),
        Err(e) => {
            eprintln!("rowline-shot: cannot decode the sprite sheet: {e}");
            None
        }
    }
}

/// The icon atlas.
fn load_icons() -> Option<rowline_assets::IconStrip> {
    for scale in [4u32, 3, 2, 1] {
        let Some(bytes) = rowline_assets::embedded_icons(scale) else {
            continue;
        };
        if let Ok(strip) =
            rowline_assets::load_icon_strip_bytes(bytes, rowline_app::commands::ICON_COUNT)
        {
            return Some(strip);
        }
    }
    None
}

/// A font: the bundled one, or a system font.
fn load_font() -> Option<Vec<u8>> {
    for dir in ["assets", "../assets", "../../assets"] {
        let p = std::path::Path::new(dir).join("font.ttf");
        if p.exists() {
            if let Ok(b) = std::fs::read(&p) {
                return Some(b);
            }
        }
    }
    rowline_assets::resolve_system_font().map(|(b, _)| b)
}
