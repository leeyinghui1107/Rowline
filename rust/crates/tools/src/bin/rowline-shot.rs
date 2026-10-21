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
}

fn main() {
    let mut out = PathBuf::from("rowline.png");
    let mut width = 1280.0_f32;
    let mut height = 900.0_f32;
    let mut scale = 1.0_f32;
    let mut light = false;
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
            "--theme" => light = value() == "light",
            "--scenario" => {
                scenario = match value().as_str() {
                    "board" => Scenario::Board,
                    "scoreboard" => Scenario::Scoreboard,
                    "about" => Scenario::About,
                    "gameover" => Scenario::GameOver,
                    _ => Scenario::New,
                }
            }
            "--help" | "-h" => {
                eprintln!(
                    "usage: rowline-shot [--out FILE] [--width N] [--height N] \
                     [--scale N] [--theme dark|light] [--scenario new|board|\
                     scoreboard|about|gameover] [--seed N]"
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
    if light {
        game.toggle_theme();
    }

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
    }

    let layout = Layout::compute(width as i32, height as i32);
    let cmds = frame(&game, layout);

    // ---- rasterise ------------------------------------------------------
    let mut canvas = SoftwareCanvas::new((width * scale) as usize, (height * scale) as usize);
    canvas.set_viewport(
        rowline_core::render::Rect::new(0.0, 0.0, width * scale, height * scale),
        scale,
    );

    if let Some(sheet) = load_sheet(scale) {
        canvas.set_sheet(sheet);
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
fn load_sheet(scale: f32) -> Option<image::RgbaImage> {
    let name = if scale >= 3.0 {
        "rowline_sheet@3x.png"
    } else if scale >= 2.0 {
        "rowline_sheet@2x.png"
    } else {
        "rowline_sheet.png"
    };
    for dir in ["assets/sheet", "../assets/sheet", "../../assets/sheet"] {
        let p = std::path::Path::new(dir).join(name);
        if p.exists() {
            if let Ok(img) = image::open(&p) {
                return Some(img.to_rgba8());
            }
        }
    }
    eprintln!("rowline-shot: the sprite sheet is missing; run 'make sheet'");
    None
}

/// The icon atlas.
fn load_icons() -> Option<rowline_assets::IconStrip> {
    for dir in ["assets/icons", "../assets/icons", "../../assets/icons"] {
        for name in ["icons@3x.png", "icons@2x.png", "icons.png"] {
            let p = std::path::Path::new(dir).join(name);
            if p.exists() {
                if let Ok(strip) =
                    rowline_assets::load_icon_strip(&p, rowline_app::commands::ICON_COUNT)
                {
                    return Some(strip);
                }
            }
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
