//! The game: state machine, input handling and the frame loop.
//!
//! The game object owns the engine and the settings and knows how to turn
//! events into engine operations.  It does **not** know how to draw (that is
//! `rowline_core::view`) nor how to read files (that is `rowline-platform`).

use rowline_core::engine::Engine;
use rowline_core::input::{InputEvent, MouseButton, Point, Size};
use rowline_core::layout::Layout;
use rowline_core::render::RenderCmd;
use rowline_core::state::{GameState, Phase};
use std::time::{Duration, Instant};

use rowline_core::{RuleModel, ScoreRecord, Settings};

use rowline_theme::DesignSystem;

use crate::commands::Command;

/// How long one dissolve step lasts.
///
/// The original's clear routine calls `Sleep(50)` once per pass
/// (`0x401C48`), and the frame burst shows two passes before the cells are
/// empty (`C:\bb\shots\clear5`, 30 ms sampling).
const CLEAR_STEP: Duration = Duration::from_millis(50);

/// How long the winner's figure animates after a clear.
const WIN_ANIM: Duration = Duration::from_millis(420);

/// Time spent travelling one cell of the move path.
///
/// The original's `clear5` burst shows the ball advancing one cell every
/// ~60 ms (docs/11 section 5f).  The animation here uses the same step, with
/// a floor so that even a one-cell move is visible instead of a teleport.
const MOVE_STEP: Duration = Duration::from_millis(60);
/// The shortest total move animation.
const MOVE_MIN: Duration = Duration::from_millis(180);
/// How long the pick-up animation of a newly selected ball lasts.
const SELECT_ANIM: Duration = Duration::from_millis(180);
/// How long the "new record" celebration lasts.
const RECORD_ANIM: Duration = Duration::from_millis(900);

/// A ball gliding along its 4-connected path.
///
/// The engine move is deferred until the animation finishes, so the source
/// ball stays on the board and the renderer covers it with the empty plate
/// while drawing the traveller.
struct MoveAnimation {
    /// Visited cells, including source and destination.
    path: Vec<usize>,
    /// The moving ball's colour.
    colour: u8,
    /// When the animation started.
    started: Instant,
}

impl MoveAnimation {
    fn duration(&self) -> Duration {
        (MOVE_STEP * self.path.len().saturating_sub(1) as u32).max(MOVE_MIN)
    }
}

/// A clear that is still dissolving on screen.
struct ClearAnimation {
    /// When the move completed.
    started: Instant,
    /// `(cell, colour)` for every eliminated ball.
    cells: Vec<(usize, u8)>,
}

/// The application state.
pub struct Game {
    engine: Engine,
    settings: Settings,
    scores: Vec<ScoreRecord>,
    selected: Option<usize>,
    path: Vec<usize>,
    phase: Phase,
    quit: bool,
    /// The visual design system (palette, spacing, motion, accessibility).
    pub design: DesignSystem,
    /// The menu model.
    pub menu: rowline_core::menu::MenuModel,
    /// The decorative particle system.
    pub effects: rowline_core::effects::EffectSystem,
    /// `true` when no move is left and the game is over.
    pub game_over: bool,
    /// `true` once the end-of-game message has been acknowledged.
    pub game_over_ack: bool,
    /// The overlay the user is interacting with.
    pub overlay: Overlay,
    /// The name being typed in the name-entry dialog.
    pub name_input: String,
    /// `true` while the prefilled default name is selected, so the first
    /// keystroke or Backspace replaces it instead of appending.
    pub name_select_all: bool,
    /// The integer scale of the sprite sheet the renderer loaded.
    sheet_scale: u32,
    /// The cell under the pointer, for the hover feedback (re-design).
    hovered: Option<usize>,
    /// The clear currently dissolving, if any.
    clear_anim: Option<ClearAnimation>,
    /// When the winning figure started its victory animation.
    win_anim: Option<Instant>,
    /// A ball currently travelling along its path.
    move_anim: Option<MoveAnimation>,
    /// When the current selection was made, for the pick-up animation.
    select_anim: Option<(usize, Instant)>,
    /// The name length when the IME composition started.
    ime_start: Option<usize>,
    /// A monotonic clock for the idle figure animation.
    clock: Instant,
    /// When the current round first overtook the record, if it has.
    record_anim: Option<Instant>,
    /// The score the **left** figure's height represents (the record).
    king_record: u32,
    /// The current rise of each figure above the pedestal floor, in logical
    /// pixels, and where the animation is heading.
    king_rise: f32,
    king_rise_target: f32,
    challenger_rise: f32,
    challenger_rise_target: f32,
    /// The drop-to-zero-then-rise played when a new record rescales the board.
    reset_anim: Option<ScaleReset>,
}

/// A record was set: the scale is recomputed and both figures are reset.
///
/// The king first drops to the floor and then climbs to the new scale's
/// height, which makes the new record look like a height increase; the
/// challenger drops back to the floor over the same first phase.
struct ScaleReset {
    started: Instant,
    king_from: f32,
    challenger_from: f32,
    /// The record displayed before the rescale, so the counter can drop to
    /// zero and climb to the new record with the figure.
    record_from: u32,
}

/// The tallest a figure may stand above the pedestal floor.
///
/// The board leaves about 150 logical pixels between the pedestal floor and
/// the HUD, so this is the layout ceiling for the score-to-height scale.
const FIGURE_MAX_RISE: f32 = 150.0;
/// The scale is 150 % of the record ("以最高纪录的 150% 作为整体高度"), so
/// the record — and therefore the king — always stands at 2/3 of the full
/// column.
const CHALLENGER_CAP: f32 = 1.5;
/// Time constant of the gradual height animation, in seconds.
const FIGURE_TAU: f32 = 0.16;
/// First phase of a new record: both figures drop to the floor.
const RESET_DROP: Duration = Duration::from_millis(450);
/// Second phase: the king climbs to the new record's height.
const RESET_RISE: Duration = Duration::from_millis(550);

/// The height that represents `score` when `record` is the top score.
///
/// Both figures use this one function, so equal scores always stand at the
/// same height; `1.5 * record` is the full column.
fn score_to_height(score: u32, record: u32) -> f32 {
    if record == 0 {
        return 0.0;
    }
    let full = record as f32 * CHALLENGER_CAP;
    (score as f32 / full * FIGURE_MAX_RISE).min(FIGURE_MAX_RISE)
}

/// The modal overlays the game can show.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Overlay {
    /// Nothing is open.
    None,
    /// The leaderboard (总榜).
    Scoreboard,
    /// The name entry (输入你的名字).
    NameEntry,
    /// The about box.
    About,
    /// The in-app command reference.
    Help,
    /// The statistics readout.
    Statistics,
}

impl Game {
    /// A new game with the given rule model.
    pub fn new(seed: u32, model: RuleModel) -> Game {
        let mut engine = Engine::new(seed, model);
        engine.new_game();
        let scores = rowline_core::scoreboard::default_table();
        let king_record = scores.first().map(|r| r.score).unwrap_or(0);
        let king_rise = score_to_height(king_record, king_record);
        Game {
            engine,
            settings: Settings::default(),
            scores,
            selected: None,
            path: Vec::new(),
            phase: Phase::Ready,
            quit: false,
            // One fixed design system: the original Classic palette.
            design: DesignSystem::new(),
            menu: rowline_core::menu::MenuModel::new(rowline_core::menu::default_menus()),
            effects: rowline_core::effects::EffectSystem::new(0x5EED),
            game_over: false,
            game_over_ack: true,
            overlay: Overlay::None,
            name_input: rowline_core::scoreboard::default_player_name().to_string(),
            name_select_all: false,
            sheet_scale: 1,
            hovered: None,
            clear_anim: None,
            win_anim: None,
            move_anim: None,
            select_anim: None,
            ime_start: None,
            clock: Instant::now(),
            record_anim: None,
            king_record,
            king_rise,
            king_rise_target: king_rise,
            challenger_rise: 0.0,
            challenger_rise_target: 0.0,
            reset_anim: None,
        }
    }

    /// Record the integer scale of the loaded sprite sheet.
    ///
    /// The renderer samples the sheet with texel coordinates, so the view
    /// builder must multiply the source rectangles by this scale.
    pub fn set_sheet_scale(&mut self, scale: u32) {
        self.sheet_scale = scale.max(1);
    }

    /// The best score in the table.
    pub fn best_score(&self) -> u32 {
        self.scores.first().map(|r| r.score).unwrap_or(0)
    }

    /// The CPU's score -- the HUD's **left** score box (docs/11 section 5b).
    ///
    /// This is the **record**: the best entry in the high-score table, or
    /// the current round's score once it beats that record (the original
    /// shows the record climbing live).
    pub fn king_score(&self) -> u32 {
        // The record does not move while the round is played; it changes
        // only when a new record is recorded.
        self.best_score().max(self.king_record)
    }

    /// The number shown in the left score box.
    ///
    /// During a rescale it follows the king's animation: down to zero, then
    /// up to the new record.
    pub fn displayed_king_score(&self) -> u32 {
        let Some(anim) = &self.reset_anim else {
            return self.king_score();
        };
        let t = anim.started.elapsed().as_secs_f32();
        let drop = RESET_DROP.as_secs_f32();
        let rise = RESET_RISE.as_secs_f32();
        if t >= drop + rise {
            self.king_score()
        } else if t < drop {
            (anim.record_from as f32 * (1.0 - t / drop)).round() as u32
        } else {
            let p = (t - drop) / rise;
            let eased = p * p * (3.0 - 2.0 * p);
            (self.king_score() as f32 * eased).round() as u32
        }
    }

    /// `true` while the "new record" celebration runs.
    pub fn record_celebrating(&self) -> bool {
        self.record_anim.is_some()
    }

    /// The human's score -- the HUD's **right** score box.
    pub fn challenger_score(&self) -> u32 {
        self.engine.score() as u32
    }

    /// `true` while a move, dissolve, victory lift or particle is running.
    ///
    /// The event loop uses this to keep redrawing while an animation is in
    /// flight; with a plain `Wait` loop a move would freeze after its first
    /// frame because no new window event arrives.
    pub fn is_animating(&self) -> bool {
        self.move_anim.is_some()
            || self.clear_anim.is_some()
            || self.win_anim.is_some()
            || self.select_anim.is_some()
            || self.record_anim.is_some()
            || self.reset_anim.is_some()
            || (self.king_rise - self.king_rise_target).abs() > 0.5
            || (self.challenger_rise - self.challenger_rise_target).abs() > 0.5
            || !self.effects.particles().is_empty()
    }

    /// The height of each figure above the pedestal floor, in logical px.
    pub fn figure_heights(&self) -> (f32, f32) {
        (self.king_rise, self.challenger_rise)
    }

    /// Seconds since the game started, for the idle figure animation.
    pub fn clock_seconds(&self) -> f32 {
        self.clock.elapsed().as_secs_f32()
    }

    /// Recompute where the two figures should stand.
    ///
    /// The king's height represents the record; the challenger's represents
    /// the current round, capped at 150 % of the record and at the layout
    /// ceiling.
    fn update_figure_targets(&mut self) {
        // Both figures use the same scale, so equal scores stand equally
        // high.  The record is 100 % of its own scale and the column is
        // 150 % of the record, hence the constant 2/3 height for the king.
        self.king_rise_target = score_to_height(self.king_record, self.king_record);
        let score = self.engine.score().max(0) as u32;
        self.challenger_rise_target = score_to_height(score, self.king_record);
    }

    /// Snap the figure animations to their targets (offline screenshots).
    pub fn finish_figure_animation(&mut self) {
        self.reset_anim = None;
        self.update_figure_targets();
        self.king_rise = self.king_rise_target;
        self.challenger_rise = self.challenger_rise_target;
    }

    /// Advance the decorative effects and finish expired animations.
    /// Apply the in-flight move immediately.
    ///
    /// The offline screenshot tool uses this to reach the dissolve frame
    /// without waiting for the move animation.
    pub fn finish_move_animation(&mut self, layout: Layout) {
        self.apply_pending_move(layout);
    }

    /// Apply and clear the pending move, if there is one.
    fn apply_pending_move(&mut self, layout: Layout) {
        let Some(anim) = self.move_anim.take() else {
            return;
        };
        if let (Some(&src), Some(&dst)) = (anim.path.first(), anim.path.last()) {
            let (sr, sc) = rowline_core::coords(src);
            let (dr, dc) = rowline_core::coords(dst);
            let res = self
                .engine
                .move_ball(sr as i32, sc as i32, dr as i32, dc as i32);
            self.apply_move_result(res, anim.colour, layout);
        }
    }

    pub fn tick(&mut self, dt: f32, layout: Layout) {
        let now = Instant::now();
        // Finish a move that has reached the end of its path.  The engine is
        // only updated here, so the source ball stays put (and is covered by
        // the renderer) while the traveller is in flight.
        if self
            .move_anim
            .as_ref()
            .is_some_and(|a| now.saturating_duration_since(a.started) >= a.duration())
        {
            self.apply_pending_move(layout);
        }
        if self
            .clear_anim
            .as_ref()
            .is_some_and(|a| now.saturating_duration_since(a.started) >= CLEAR_STEP * 2)
        {
            self.clear_anim = None;
        }
        if self
            .win_anim
            .is_some_and(|t| now.saturating_duration_since(t) >= WIN_ANIM)
        {
            self.win_anim = None;
        }
        if self
            .select_anim
            .as_ref()
            .is_some_and(|(_, t)| now.saturating_duration_since(*t) >= SELECT_ANIM)
        {
            self.select_anim = None;
        }
        if self
            .record_anim
            .is_some_and(|t| now.saturating_duration_since(t) >= RECORD_ANIM)
        {
            self.record_anim = None;
        }
        // The figures' heights follow their scores: the king stands at the
        // record (2/3 of the column), the challenger grows with the current
        // round up to 150 % of the record.  A new record plays a two-phase
        // reset instead: both drop to the floor, then the king climbs to the
        // new scale's height.
        if let Some(anim) = self.reset_anim.take() {
            let t = now.saturating_duration_since(anim.started).as_secs_f32();
            let drop = RESET_DROP.as_secs_f32();
            let rise = RESET_RISE.as_secs_f32();
            if t >= drop + rise {
                self.king_rise = self.king_rise_target;
                self.challenger_rise = 0.0;
            } else if t < drop {
                let k = 1.0 - t / drop;
                self.king_rise = anim.king_from * k;
                self.challenger_rise = anim.challenger_from * k;
                self.reset_anim = Some(anim);
            } else {
                let p = (t - drop) / rise;
                self.king_rise = self.king_rise_target * (p * p * (3.0 - 2.0 * p));
                self.challenger_rise = 0.0;
                self.reset_anim = Some(anim);
            }
        } else {
            self.update_figure_targets();
            let k = 1.0 - (-dt / FIGURE_TAU).exp();
            self.king_rise += (self.king_rise_target - self.king_rise) * k;
            self.challenger_rise += (self.challenger_rise_target - self.challenger_rise) * k;
        }
        self.effects.update(dt);
    }

    /// Spawn the burst for a cleared cell.
    fn burst_at(&mut self, row: usize, col: usize, layout: Layout) {
        let (dx, dy) = layout.cell_origin(row as i32, col as i32);
        let colour = self
            .design
            .palette
            .balls
            .get(self.engine.board().colour_at(row, col))
            .to_hex();
        let c = rowline_core::render::Color {
            r: ((colour >> 16) & 0xFF) as u8,
            g: ((colour >> 8) & 0xFF) as u8,
            b: (colour & 0xFF) as u8,
            a: 255,
        };
        self.effects.burst(
            rowline_core::input::Point::new(dx as f32 + 18.0, dy as f32 + 18.0),
            c,
            10,
            140.0,
        );
    }

    /// The rectangle of the menu bar, in logical pixels.
    ///
    /// The bar spans the whole window, like the original's menu bar; the
    /// titles sit at its left end.
    pub fn menu_bar_rect(&self, client_w: i32) -> rowline_core::render::Rect {
        let m = &self.menu.metrics;
        rowline_core::render::Rect::new(0.0, 0.0, client_w as f32, m.bar_height)
    }

    /// The rectangle of the open dropdown, in logical pixels.
    pub fn menu_panel_rect(&self) -> Option<rowline_core::render::Rect> {
        let open = self.menu.open?;
        let m = &self.menu.metrics;
        let items = self.menu.menus.get(open)?.items.len() as f32;
        Some(rowline_core::render::Rect::new(
            open as f32 * m.title_width,
            m.bar_height,
            m.row_width,
            items * m.row_height,
        ))
    }

    /// The current settings.
    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    /// Record the window geometry in logical units, for the next launch.
    ///
    /// `x` / `y` are the outer window position (the original stores the same
    /// values in its INI); `width` / `height` are the client size.
    pub fn set_window_geometry(&mut self, x: i32, y: i32, width: u32, height: u32) {
        self.settings.window_x = x;
        self.settings.window_y = y;
        self.settings.window_width = width;
        self.settings.window_height = height;
    }

    /// The score table.
    pub fn scores(&self) -> &[ScoreRecord] {
        &self.scores
    }

    /// The engine.
    pub fn engine(&self) -> &Engine {
        &self.engine
    }

    /// The engine, mutably.
    pub fn engine_mut(&mut self) -> &mut Engine {
        &mut self.engine
    }

    /// `true` when the user asked to quit.
    pub fn quit_requested(&self) -> bool {
        self.quit
    }

    /// Build the data to persist.
    pub fn to_save(&self) -> rowline_core::save::SaveData {
        rowline_core::save::capture(self.settings.clone(), self.scores.clone())
    }

    /// Replace the settings and the score table.
    pub fn load_save(&mut self, save: rowline_core::save::SaveData) {
        self.settings = save.settings;
        // An empty table means "nothing was ever recorded", so the default
        // entry is put back -- the original always shows the CPU in the table.
        self.scores = if save.scores.is_empty() {
            rowline_core::scoreboard::default_table()
        } else {
            save.scores
        };
        self.king_record = self.scores.iter().map(|r| r.score).max().unwrap_or(0);
        // Start with the figures already at rest.
        self.finish_figure_animation();
    }

    /// Snapshot the state for the presentation layers.
    pub fn snapshot(&self, layout: Layout) -> GameState {
        GameState {
            phase: self.phase,
            score: self.engine.score(),
            best_score: self.best_score() as i64,
            board: self.engine.board().clone(),
            selected: self.selected,
            path: self.path.clone(),
            next_group: self.engine.next_group(),
            sheet_scale: self.sheet_scale,
            layout,
        }
    }

    /// Handle one input event.
    pub fn handle_event(&mut self, ev: &InputEvent, layout: Layout) {
        // The end-of-game message is modal: it must be acknowledged before
        // anything else happens.  The original then offers the name entry
        // (and the leaderboard), exactly once.
        if self.game_over && !self.game_over_ack {
            let acknowledge = matches!(ev, InputEvent::KeyPressed { .. })
                || matches!(
                    ev,
                    InputEvent::MousePressed {
                        button: MouseButton::Left,
                        ..
                    }
                );
            if acknowledge {
                self.ack_game_over();
                return;
            }
        }
        match ev {
            InputEvent::KeyPressed { key, modifiers } => {
                // The name entry captures the typing.  While an IME
                // composition is active the physical keys are pinyin, which
                // the IME renders itself; they must not also enter the name.
                if self.overlay == Overlay::NameEntry {
                    if self.ime_start.is_none() {
                        self.name_key(*key);
                    } else if matches!(key, rowline_core::input::Key::Escape) {
                        self.dismiss_overlay();
                    }
                    return;
                }
                // The menu has priority over the game commands.
                if self.menu_key(*key) {
                    return;
                }
                // The original's Win32 accelerators: Alt+F / Alt+O open the
                // two menus and Alt+F9 quits.
                if modifiers.alt {
                    use rowline_core::input::Key;
                    match key {
                        Key::KeyF => {
                            self.open_menu(0);
                            return;
                        }
                        Key::KeyO => {
                            self.open_menu(1);
                            return;
                        }
                        Key::F9 => {
                            self.quit = true;
                            return;
                        }
                        _ => {}
                    }
                }
                if let Some(cmd) = Command::from_key(*key) {
                    self.run_command(cmd);
                }
            }
            InputEvent::MousePressed { pos, button, .. } => {
                if *button == MouseButton::Left {
                    // Modal overlays swallow every click.
                    if self.overlay_click(*pos, layout) {
                        return;
                    }
                    // The menu has priority over the board.
                    if self.menu_click(*pos) {
                        return;
                    }
                    self.click_at(*pos, layout);
                }
            }
            InputEvent::MouseMoved { pos } => {
                self.menu_hover(*pos);
                self.update_hover(*pos, layout);
            }
            InputEvent::ImePreedit { text } => {
                // The first preedit marks where the raw pinyin starts; when
                // the composition commits we drop those letters and insert
                // the composed characters instead.
                if self.overlay == Overlay::NameEntry {
                    if text.is_empty() {
                        self.ime_start = None;
                    } else {
                        if self.name_select_all {
                            self.name_input.clear();
                            self.name_select_all = false;
                        }
                        if self.ime_start.is_none() {
                            self.ime_start = Some(self.name_input.len());
                        }
                    }
                }
            }
            InputEvent::TextInput { text } => {
                if self.overlay != Overlay::NameEntry {
                    return;
                }
                let committed: Vec<char> = text.chars().filter(|c| !c.is_control()).collect();
                if self.name_select_all && !text.is_ascii() && !committed.is_empty() {
                    // The default name was selected: the first IME commit
                    // replaces it.  ASCII text is ignored here because the
                    // physical key path already inserts it (accepting both
                    // would duplicate every Latin character).
                    self.name_input.clear();
                    self.name_select_all = false;
                    self.ime_start = None;
                    for ch in committed {
                        if self.name_input.chars().count() < 24 {
                            self.name_input.push(ch);
                        }
                    }
                    return;
                }
                // A commit replaces the raw pinyin typed during composition.
                if let Some(start) = self.ime_start.take() {
                    self.name_input.truncate(start);
                    for ch in text.chars().filter(|c| !c.is_control()) {
                        if self.name_input.chars().count() < 24 {
                            self.name_input.push(ch);
                        }
                    }
                } else if !text.is_ascii() {
                    // Some IMEs commit without reporting a preedit.  The
                    // physical keys have already typed the raw pinyin, so
                    // strip the trailing Latin letters before inserting the
                    // composed characters.
                    while self
                        .name_input
                        .chars()
                        .last()
                        .is_some_and(|c| c.is_ascii_alphabetic())
                    {
                        self.name_input.pop();
                    }
                    for ch in text.chars().filter(|c| !c.is_control()) {
                        if self.name_input.chars().count() < 24 {
                            self.name_input.push(ch);
                        }
                    }
                }
            }
            InputEvent::QuitRequested => self.quit = true,
            _ => {}
        }
    }

    /// Open `overlay`, or close it when it is already the active one.
    fn toggle_overlay(&mut self, overlay: Overlay) {
        self.overlay = if self.overlay == overlay {
            Overlay::None
        } else {
            overlay
        };
    }

    /// Execute a menu command.
    pub fn run_command(&mut self, cmd: Command) {
        match cmd {
            Command::NEW_GAME => self.start_new_game(),
            Command::TOGGLE_STATISTICS => self.toggle_overlay(Overlay::Statistics),
            Command::EXIT => self.quit = true,
            Command::SHOW_HIGH_SCORES => {
                self.overlay = Overlay::Scoreboard;
            }
            Command::ABOUT => self.toggle_overlay(Overlay::About),
            Command::HELP => self.toggle_overlay(Overlay::Help),
            // `Command` is an open tuple struct, so an unknown id must be
            // tolerated rather than treated as a programming error.
            _ => {}
        }
    }

    /// `true` when the click was consumed by the menu.
    fn menu_click(&mut self, pos: Point) -> bool {
        let m = self.menu.metrics;
        // The menu bar.
        if pos.y >= 0.0 && pos.y < m.bar_height {
            let idx = (pos.x / m.title_width).floor();
            if idx >= 0.0 && (idx as usize) < self.menu.menus.len() {
                self.menu.toggle(idx as usize);
                return true;
            }
            self.menu.close();
            return true;
        }
        // The open dropdown.
        if let Some(panel) = self.menu_panel_rect() {
            if panel.contains(pos) {
                let cmd = self.menu.click(pos.y - panel.y);
                if let Some(id) = cmd {
                    self.run_command(crate::commands::Command(id));
                }
                return true;
            }
        }
        // A click elsewhere closes an open menu and is swallowed.
        if !self.menu.is_closed() {
            self.menu.close();
            return true;
        }
        false
    }

    /// Update the menu hover state.
    fn menu_hover(&mut self, pos: Point) {
        // Hover never opens a closed menu: the original opens on click and
        // only switches between menus while one is already open.  Opening on
        // hover would also make the following click toggle it straight shut.
        if self.menu.is_closed() {
            return;
        }
        let m = self.menu.metrics;
        if pos.y >= 0.0 && pos.y < m.bar_height {
            let idx = (pos.x / m.title_width).floor();
            if idx >= 0.0
                && (idx as usize) < self.menu.menus.len()
                && self.menu.open != Some(idx as usize)
            {
                self.menu.open = Some(idx as usize);
                self.menu.hover = self.menu.first_selectable(idx as usize);
            }
            {}
            return;
        }
        if let Some(panel) = self.menu_panel_rect() {
            if panel.contains(pos) {
                self.menu.hover_at(pos.y - panel.y);
            }
        }
    }

    /// Open a top-level menu by index (used by the Alt+F / Alt+O mnemonics).
    fn open_menu(&mut self, index: usize) {
        if self.menu.open != Some(index) {
            self.menu.close();
            self.menu.toggle(index);
        }
    }

    /// `true` when the key was consumed by the menu.
    fn menu_key(&mut self, key: rowline_core::input::Key) -> bool {
        use rowline_core::input::Key;
        match key {
            Key::F10 => {
                self.menu.open_bar();
                true
            }
            Key::Escape => {
                if !self.menu.is_closed() {
                    self.menu.close();
                    true
                } else if self.overlay != Overlay::None {
                    self.dismiss_overlay();
                    true
                } else {
                    false
                }
            }
            Key::ArrowUp if !self.menu.is_closed() => {
                self.menu.move_hover(-1);
                true
            }
            Key::ArrowDown if !self.menu.is_closed() => {
                self.menu.move_hover(1);
                true
            }
            Key::ArrowLeft if !self.menu.is_closed() => {
                self.menu.next_menu(-1);
                true
            }
            Key::ArrowRight if !self.menu.is_closed() => {
                self.menu.next_menu(1);
                true
            }
            Key::Enter if !self.menu.is_closed() => {
                if let Some(id) = self.menu.activate() {
                    self.run_command(crate::commands::Command(id));
                }
                true
            }
            _ => false,
        }
    }

    /// Handle a key press while the name entry is open.
    fn name_key(&mut self, key: rowline_core::input::Key) {
        use rowline_core::input::Key;
        match key {
            Key::Enter => self.accept_name(),
            Key::Backspace => {
                if self.name_select_all {
                    // The prefilled default is selected: delete it whole.
                    self.name_input.clear();
                    self.name_select_all = false;
                } else {
                    self.name_input.pop();
                }
            }
            Key::Escape => self.dismiss_overlay(),
            _ => {
                // Only printable characters are accepted; the name is capped.
                if let Some(ch) = key_char(key) {
                    if self.name_select_all {
                        self.name_input.clear();
                        self.name_select_all = false;
                    }
                    if self.name_input.chars().count() < 24 {
                        self.name_input.push(ch);
                    }
                }
            }
        }
    }

    /// Handle a click at a client position.
    ///
    /// Public so that the offline screenshot tool can drive the same
    /// two-click interaction as the application.
    pub fn click_at(&mut self, pos: Point, layout: Layout) {
        let Some((r, c)) = layout.hit_cell(pos.x as i32, pos.y as i32) else {
            return;
        };
        let cell = rowline_core::index(r as usize, c as usize);
        // A move already in flight owns the board.
        if self.move_anim.is_some() {
            return;
        }
        let occupied = self.engine.board().colour_at(r as usize, c as usize) != 0;
        // Clicking a ball always selects it: a second ball switches the
        // selection (the original's two-click model never tries to move onto
        // an occupied cell), and clicking the selected ball clears it.
        if occupied {
            if self.selected == Some(cell) {
                self.selected = None;
                self.select_anim = None;
                self.path.clear();
                self.phase = Phase::Ready;
            } else {
                self.selected = Some(cell);
                self.select_anim = Some((cell, Instant::now()));
                self.path.clear();
                self.phase = Phase::Selected;
            }
            return;
        }
        match self.selected {
            None => {}
            Some(prev) => {
                if prev == cell {
                    self.selected = None;
                    self.path.clear();
                    self.phase = Phase::Ready;
                    return;
                }
                let path = rowline_core::rules::find_path(self.engine.board(), prev, cell);
                if path.is_empty() {
                    self.selected = None;
                    self.path.clear();
                    self.phase = Phase::Ready;
                    return;
                }
                let (pr, pc) = rowline_core::coords(prev);
                let moving_colour = self.engine.board().colour_at(pr, pc);
                self.selected = None;
                self.select_anim = None;
                self.path.clear();
                self.hovered = None;
                self.phase = Phase::Animating;
                self.move_anim = Some(MoveAnimation {
                    path,
                    colour: moving_colour,
                    started: Instant::now(),
                });
            }
        }
    }

    /// Apply the engine's move result: score, clear animation and game over.
    fn apply_move_result(
        &mut self,
        res: rowline_core::MoveResult,
        moving_colour: u8,
        layout: Layout,
    ) {
        if res.status != Some(rowline_core::MoveStatus::Moved) {
            self.phase = Phase::Ready;
            return;
        }
        // A new record: the left score box tracks the best score, and the
        // king celebrates the first time the round overtakes it.
        let record = self.best_score();
        if self.engine.score() as u32 > record && self.record_anim.is_none() {
            self.record_anim = Some(Instant::now());
        }
        self.phase = if res.group.is_empty() {
            Phase::Ready
        } else {
            Phase::Animating
        };
        if res.group.is_empty() {
            self.clear_anim = None;
        } else {
            let started = Instant::now();
            self.clear_anim = Some(ClearAnimation {
                started,
                cells: res
                    .group
                    .iter()
                    .map(|&cell| (cell, moving_colour))
                    .collect(),
            });
            self.win_anim = Some(started);
        }
        if !self.engine.has_legal_move() {
            self.game_over = true;
            self.game_over_ack = false;
        }
        for cell in &res.group {
            let (gr, gc) = rowline_core::coords(*cell);
            self.burst_at(gr, gc, layout);
        }
    }

    /// Update the cell under the pointer and the path preview.
    ///
    /// The original gives no board hover feedback (it is a two-click game);
    /// the re-implementation adds a subtle ring and previews the shortest
    /// 4-connected route once a ball is selected.
    fn update_hover(&mut self, pos: Point, layout: Layout) {
        if self.move_anim.is_some() || self.overlay != Overlay::None {
            self.hovered = None;
            self.path.clear();
            return;
        }
        self.hovered = layout
            .hit_cell(pos.x as i32, pos.y as i32)
            .map(|(r, c)| rowline_core::index(r as usize, c as usize));
        self.path.clear();
        let (Some(sel), Some(hover)) = (self.selected, self.hovered) else {
            return;
        };
        if hover == sel || self.engine.board().cells()[hover] != 0 {
            return;
        }
        self.path = rowline_core::rules::find_path(self.engine.board(), sel, hover);
    }

    /// The cell under the pointer.
    pub fn hovered(&self) -> Option<usize> {
        self.hovered
    }

    /// Acknowledge the end-of-game message.
    ///
    /// This mirrors `0x4032E2`: the original shows `游戏结束!`, and when the
    /// player dismisses it, `0x402CB1` inserts the score.  If the score makes
    /// the table, the name entry opens; the leaderboard follows on accept.
    pub fn ack_game_over(&mut self) {
        self.game_over_ack = true;
        let score = self.engine().score() as u32;
        let sb = rowline_core::scoreboard::Scoreboard::new(self.scores().to_vec());
        if sb.qualifies(score) {
            self.offer_name_entry();
        }
    }

    /// Called when the player makes the table: ask for a name.
    pub fn offer_name_entry(&mut self) {
        self.overlay = Overlay::NameEntry;
        self.name_input = rowline_core::scoreboard::default_player_name().to_string();
        self.name_select_all = true;
    }

    /// Accept the typed name and record it, then show the leaderboard.
    ///
    /// The original writes the row and then shows the table (`0x402D4A`),
    /// which is why accepting a name lands on the leaderboard.
    pub fn accept_name(&mut self) {
        self.name_select_all = false;
        let name = self.name_input.trim().to_string();
        let name = if name.is_empty() {
            rowline_core::scoreboard::default_player_name().to_string()
        } else {
            name
        };
        let score = self.engine().score() as u32;
        // A round worth nothing is not worth a row in the table.
        if score > 0 {
            self.scores.push(ScoreRecord { name, score });
            self.scores.sort_by_key(|r| std::cmp::Reverse(r.score));
            self.scores.truncate(rowline_core::scoreboard::ROWS);
        }
        self.overlay = Overlay::Scoreboard;
    }

    /// Dismiss the current overlay.
    pub fn dismiss_overlay(&mut self) {
        self.overlay = Overlay::None;
        self.name_select_all = false;
    }

    /// Clear the leaderboard (the 清除 button).
    pub fn clear_scores(&mut self) {
        self.scores = rowline_core::scoreboard::default_table();
    }

    /// Start a fresh round and reset every interaction state.
    ///
    /// A round that beat the record first raises the king's record, so the
    /// new targets make the king climb to the challenger's height while the
    /// challenger drops back to the floor -- both animate at once.
    pub fn start_new_game(&mut self) {
        let score = self.engine.score().max(0) as u32;
        let new_record = score > self.king_record;
        // The counter starts where it is shown now (the table may already
        // hold the new record if the name was accepted).
        let record_from = self.king_score();
        if new_record {
            self.king_record = score;
        }
        let king_from = self.king_rise;
        let challenger_from = self.challenger_rise;
        self.engine_mut().new_game();
        self.selected = None;
        self.path.clear();
        self.hovered = None;
        self.phase = Phase::Ready;
        self.game_over = false;
        self.game_over_ack = true;
        self.overlay = Overlay::None;
        self.clear_anim = None;
        self.win_anim = None;
        self.move_anim = None;
        self.select_anim = None;
        self.ime_start = None;
        self.record_anim = None;
        self.update_figure_targets();
        self.reset_anim = None;
        if new_record {
            // The scale just changed (the record moved).  Drop both figures
            // to the floor first, then raise the king to the new scale's
            // height: it looks like the record grew.
            self.reset_anim = Some(ScaleReset {
                started: Instant::now(),
                king_from,
                challenger_from,
                record_from,
            });
        }
    }

    /// Handle a click on a modal overlay; `true` when it was consumed.
    ///
    /// The dialogs are drawn by the app, not by the OS, so their buttons need
    /// explicit hit-testing.  Without this the 确定(O) button in the name
    /// entry (and the buttons of the leaderboard) were dead.
    fn overlay_click(&mut self, pos: Point, layout: Layout) -> bool {
        match self.overlay {
            Overlay::NameEntry => {
                let (panel, field, ok) = name_entry_rects(layout);
                if close_rect(panel).contains(pos) {
                    self.dismiss_overlay();
                } else if field.contains(pos) {
                    self.name_select_all = false;
                } else if ok.contains(pos) {
                    self.accept_name();
                }
                // Modal: never let a click reach the board behind it.
                true
            }
            Overlay::Scoreboard => {
                let (clear, ok) = scoreboard_buttons(scoreboard_rects(layout));
                let (panel, _) = scoreboard_rects(layout);
                if close_rect(panel).contains(pos) {
                    self.dismiss_overlay();
                } else if clear.contains(pos) {
                    self.clear_scores();
                } else if ok.contains(pos) {
                    self.dismiss_overlay();
                    // The original starts a fresh round once the table is
                    // acknowledged at the end of a game.
                    if self.game_over {
                        self.start_new_game();
                    }
                }
                true
            }
            Overlay::About => {
                let (panel, button) = about_rects(layout);
                if close_rect(panel).contains(pos) || button.contains(pos) || !panel.contains(pos) {
                    self.dismiss_overlay();
                }
                true
            }
            Overlay::Help => {
                let (panel, button) = help_rects(layout);
                if close_rect(panel).contains(pos) || button.contains(pos) || !panel.contains(pos) {
                    self.dismiss_overlay();
                }
                true
            }
            Overlay::Statistics => {
                let (panel, button) = stats_rects(layout);
                if close_rect(panel).contains(pos) || button.contains(pos) || !panel.contains(pos) {
                    self.dismiss_overlay();
                }
                true
            }
            Overlay::None => false,
        }
    }

    /// Show or hide the end-of-game dialog.
    ///
    /// The game sets this itself when no move is left; the offline
    /// screenshot tool uses it to render the dialog.
    pub fn set_game_over(&mut self, over: bool) {
        self.game_over = over;
        self.game_over_ack = !over;
    }
}

/// Build the render commands for the current state.
///
/// The board itself is drawn by the shared view builder; the HUD and the
/// dialogs are drawn here in the Classic style: square controls, beveled
/// surfaces and black-on-grey text.
pub fn frame(game: &Game, layout: Layout) -> Vec<RenderCmd> {
    let mut cmds = rowline_core::view::build_frame(&game.snapshot(layout), &view_style(game));

    // The travelling ball, the pick-up pulse, then the hover feedback and
    // the clear dissolve.
    cmds.extend(move_animation_commands(game, layout));
    cmds.extend(selection_animation_commands(game, layout));
    cmds.extend(hover_commands(game, layout));
    if let Some(anim) = &game.clear_anim {
        cmds.extend(clear_animation_commands(game, layout, anim));
    }

    // The HUD bar sits above the board, exactly as in the original.
    cmds.extend(hud_commands(game, layout));

    // The two figures flank the board.
    cmds.extend(figure_commands(game, layout));

    cmds.extend(menu_commands(game, layout));

    // ---- the decorative particles ---------------------------------------
    for p in game.effects.particles() {
        let a = (p.opacity() * 255.0) as u8;
        cmds.push(RenderCmd::RoundedRect {
            rect: rowline_core::render::Rect::new(p.pos.x, p.pos.y, p.size, p.size),
            radius: p.size * 0.5,
            color: rowline_core::render::Color {
                r: p.color.r,
                g: p.color.g,
                b: p.color.b,
                a,
            },
        });
    }

    // ---- the end-of-game dialog ----------------------------------------
    if game.game_over && !game.game_over_ack {
        cmds.push(scrim(game, layout));
        cmds.extend(game_over_dialog(game, layout));
    }

    // ---- the modal overlays -------------------------------------------
    if game.overlay != Overlay::None {
        cmds.push(scrim(game, layout));
    }
    match game.overlay {
        Overlay::Scoreboard => cmds.extend(scoreboard_overlay(game, layout)),
        Overlay::NameEntry => cmds.extend(name_entry_overlay(game, layout)),
        Overlay::About => cmds.extend(about_overlay(game, layout)),
        Overlay::Help => cmds.extend(help_overlay(game, layout)),
        Overlay::Statistics => cmds.extend(stats_panel(game, layout)),
        Overlay::None => {}
    }

    // The original's window has a raised bevel around the client area.
    // The layout reserves the bottom rows for it.
    cmds.extend(client_frame_commands(layout));

    cmds
}

/// The raised bevel around the whole client area.
fn client_frame_commands(layout: Layout) -> Vec<RenderCmd> {
    if layout.client_w < 8 || layout.client_h < 8 {
        return Vec::new();
    }
    let w = layout.client_w as f32;
    let h = layout.client_h as f32;
    let mut cmds = Vec::new();
    for i in 0..2 {
        let i = i as f32;
        cmds.push(RenderCmd::Frame {
            rect: rowline_core::render::Rect::new(i, i, w - 2.0 * i, h - 2.0 * i),
            style: rowline_core::render::FrameStyle::Raised,
        });
    }
    cmds.push(RenderCmd::Frame {
        rect: rowline_core::render::Rect::new(2.0, 2.0, w - 4.0, h - 4.0),
        style: rowline_core::render::FrameStyle::Flat,
    });
    cmds
}

/// Append one ball at a centre point.
///
/// The product uses the original atlas sprite for the ball itself.
fn ball_at(cmds: &mut Vec<RenderCmd>, game: &Game, colour: u8, cx: f32, cy: f32, d: f32) {
    cmds.push(RenderCmd::Sprite {
        sheet: rowline_core::render::SpriteSheetId::MAIN,
        src: rowline_core::view::ball_inner_src(colour, game.sheet_scale),
        dst: Point::new(cx - d * 0.5, cy - d * 0.5),
        size: Size::new(d, d),
        tint: rowline_core::render::Color::WHITE,
    });
}

/// Draw the ball gliding along its move path.
///
/// The engine move happens when the animation finishes (`Game::tick`), so
/// the board still holds the source ball; the source plate is redrawn here
/// to cover it, and the traveller is interpolated between cell centres.
fn move_animation_commands(game: &Game, layout: Layout) -> Vec<RenderCmd> {
    let Some(anim) = &game.move_anim else {
        return Vec::new();
    };
    if anim.path.len() < 2 {
        return Vec::new();
    }
    let mut cmds = Vec::new();

    // Hide the source ball (the engine has not moved it yet).
    let (sr, sc) = rowline_core::coords(anim.path[0]);
    let (sx, sy) = layout.cell_origin(sr as i32, sc as i32);
    cmds.push(RenderCmd::Sprite {
        sheet: rowline_core::render::SpriteSheetId::MAIN,
        src: rowline_core::view::ball_src(0, game.sheet_scale),
        dst: Point::new(sx as f32, sy as f32),
        size: Size::new(36.0, 36.0),
        tint: rowline_core::render::Color::WHITE,
    });

    // Highlight the route, like the original's moving path line.
    let base: rowline_core::render::Color = game.design.palette.selection.into();
    let highlight = rowline_core::render::Color { a: 110, ..base };
    for &cell in &anim.path {
        let (r, c) = rowline_core::coords(cell);
        let (x, y) = layout.cell_origin(r as i32, c as i32);
        cmds.push(RenderCmd::Outline {
            rect: rowline_core::render::Rect::new(x as f32 + 1.0, y as f32 + 1.0, 34.0, 34.0),
            color: highlight,
        });
    }

    // Interpolate cell by cell, so the ball follows the path's turns.
    let total = anim.duration().as_secs_f32().max(0.001);
    let elapsed = anim.started.elapsed().as_secs_f32();
    let segs = (anim.path.len() - 1) as f32;
    let progress = (elapsed / total * segs).clamp(0.0, segs);
    let seg = (progress.floor() as usize).min(anim.path.len() - 2);
    let frac = progress - seg as f32;
    let (r0, c0) = rowline_core::coords(anim.path[seg]);
    let (r1, c1) = rowline_core::coords(anim.path[seg + 1]);
    let (x0, y0) = layout.cell_origin(r0 as i32, c0 as i32);
    let (x1, y1) = layout.cell_origin(r1 as i32, c1 as i32);
    let cx = (x0 as f32 + 18.0) + (x1 - x0) as f32 * frac;
    let cy = (y0 as f32 + 18.0) + (y1 - y0) as f32 * frac;

    ball_at(&mut cmds, game, anim.colour, cx, cy, 30.0);
    cmds
}

/// The pick-up pulse drawn when a ball is selected.
///
/// A ring expands and fades over `SELECT_ANIM`, so the click has immediate
/// feedback even before the ball moves.
fn selection_animation_commands(game: &Game, layout: Layout) -> Vec<RenderCmd> {
    let Some((cell, started)) = game.select_anim else {
        return Vec::new();
    };
    let t = started.elapsed().as_secs_f32() / SELECT_ANIM.as_secs_f32();
    if t >= 1.0 {
        return Vec::new();
    }
    let (r, c) = rowline_core::coords(cell);
    let (x, y) = layout.cell_origin(r as i32, c as i32);
    let base: rowline_core::render::Color = game.design.palette.selection.into();
    let alpha = ((1.0 - t) * 255.0) as u8;
    // The selected ball also hops up and settles back down.
    let hop = (t * std::f32::consts::PI).sin() * 4.0;
    let colour = game.engine().board().colour_at(r, c);
    let mut cmds = Vec::new();
    if colour != 0 {
        cmds.push(RenderCmd::Sprite {
            sheet: rowline_core::render::SpriteSheetId::MAIN,
            src: rowline_core::view::ball_src(0, game.sheet_scale),
            dst: Point::new(x as f32, y as f32),
            size: Size::new(36.0, 36.0),
            tint: rowline_core::render::Color::WHITE,
        });
        ball_at(
            &mut cmds,
            game,
            colour,
            x as f32 + 18.0,
            y as f32 + 18.0 - hop,
            22.0,
        );
    }
    // The ring grows from the cell edge outwards.
    let grow = 36.0 + 12.0 * t;
    let offset = (grow - 36.0) * 0.5;
    cmds.push(RenderCmd::Outline {
        rect: rowline_core::render::Rect::new(x as f32 - offset, y as f32 - offset, grow, grow),
        color: rowline_core::render::Color { a: alpha, ..base },
    });
    cmds
}

/// The hover ring under the pointer, when the board is interactive.
fn hover_commands(game: &Game, layout: Layout) -> Vec<RenderCmd> {
    if game.overlay != Overlay::None
        || game.move_anim.is_some()
        || (game.game_over && !game.game_over_ack)
        || !game.menu.is_closed()
    {
        return Vec::new();
    }
    let Some(cell) = game.hovered else {
        return Vec::new();
    };
    let (r, c) = rowline_core::coords(cell);
    let (dx, dy) = layout.cell_origin(r as i32, c as i32);
    vec![RenderCmd::Outline {
        rect: rowline_core::render::Rect::new(dx as f32 + 1.0, dy as f32 + 1.0, 34.0, 34.0),
        color: game.design.palette.accent.into(),
    }]
}

/// Draw the two-step dissolve recovered from `C:\bb\shots\clear5`.
///
/// The original's clear routine makes two passes with `Sleep(50)` between
/// them: the first pass draws a sparse pattern over every eliminated ball,
/// the second clears the cells and draws the empty plate.  The re-design
/// approximates the pattern with a coarse 3×3 checker; the timing matches.
fn clear_animation_commands(game: &Game, layout: Layout, anim: &ClearAnimation) -> Vec<RenderCmd> {
    let mut cmds = Vec::new();
    let phase = (anim.started.elapsed().as_millis() / CLEAR_STEP.as_millis()) as usize;
    if phase > 1 {
        return cmds;
    }
    let p = &game.design.palette;
    for &(cell, colour) in &anim.cells {
        let (r, c) = rowline_core::coords(cell);
        let (dx, dy) = layout.cell_origin(r as i32, c as i32);
        let colour = p.balls.get(colour).into();
        let keep = |i: usize, j: usize| {
            if phase == 0 {
                (i + j).is_multiple_of(2)
            } else {
                // Three of the nine fragments survive the second pass.
                matches!((i, j), (0, 0) | (1, 2) | (2, 1))
            }
        };
        for j in 0..3 {
            for i in 0..3 {
                if !keep(i, j) {
                    continue;
                }
                cmds.push(RenderCmd::RoundedRect {
                    rect: rowline_core::render::Rect::new(
                        dx as f32 + 3.0 + i as f32 * 10.0,
                        dy as f32 + 3.0 + j as f32 * 10.0,
                        10.0,
                        10.0,
                    ),
                    radius: 2.0,
                    color: colour,
                });
            }
        }
    }
    cmds
}

/// Geometry of the help dialog: the card and its footer button.
fn help_rects(layout: Layout) -> (rowline_core::render::Rect, rowline_core::render::Rect) {
    let rows = crate::commands::ALL.len() as f32;
    let panel = dialog_rect(layout, 420.0, 46.0 + 16.0 + rows * 30.0 + 64.0);
    (panel, dialog_button_rect(panel))
}

/// The in-app command reference (the redesign's only help window).
fn help_overlay(game: &Game, layout: Layout) -> Vec<RenderCmd> {
    let ds = &game.design;
    let p = &ds.palette;
    let (panel, _button) = help_rects(layout);
    let mut cmds = dialog_chrome(game, panel, "\u{5e2e}\u{52a9}");

    let body = ds.text_size(rowline_theme::TextRole::Body) as u16;
    for (i, cmd) in crate::commands::ALL.iter().enumerate() {
        let y = panel.y + DIALOG_TITLE_H + 12.0 + i as f32 * 30.0;
        cmds.push(RenderCmd::Text {
            text: cmd.label().to_string(),
            at: rowline_core::input::Point::new(panel.x + 24.0, y),
            size: body,
            color: p.text_primary.into(),
        });
        let hint = cmd.shortcut();
        if !hint.is_empty() {
            let width = 8.0 * hint.chars().count() as f32;
            cmds.push(RenderCmd::Text {
                text: hint.to_string(),
                at: rowline_core::input::Point::new(panel.x + panel.width - 24.0 - width, y),
                size: body,
                color: p.text_tertiary.into(),
            });
        }
    }

    cmds.extend(dialog_button(game, panel));
    cmds
}

/// The character a key produces, if it is a printable one.
fn key_char(key: rowline_core::input::Key) -> Option<char> {
    use rowline_core::input::Key;
    Some(match key {
        Key::KeyA => 'a',
        Key::KeyB => 'b',
        Key::KeyC => 'c',
        Key::KeyD => 'd',
        Key::KeyE => 'e',
        Key::KeyF => 'f',
        Key::KeyG => 'g',
        Key::KeyH => 'h',
        Key::KeyI => 'i',
        Key::KeyJ => 'j',
        Key::KeyK => 'k',
        Key::KeyL => 'l',
        Key::KeyM => 'm',
        Key::KeyN => 'n',
        Key::KeyO => 'o',
        Key::KeyP => 'p',
        Key::KeyQ => 'q',
        Key::KeyR => 'r',
        Key::KeyS => 's',
        Key::KeyT => 't',
        Key::KeyU => 'u',
        Key::KeyV => 'v',
        Key::KeyW => 'w',
        Key::KeyX => 'x',
        Key::KeyY => 'y',
        Key::KeyZ => 'z',
        Key::Digit0 => '0',
        Key::Digit1 => '1',
        Key::Digit2 => '2',
        Key::Digit3 => '3',
        Key::Digit4 => '4',
        Key::Digit5 => '5',
        Key::Digit6 => '6',
        Key::Digit7 => '7',
        Key::Digit8 => '8',
        Key::Digit9 => '9',
        Key::Space => ' ',
        _ => return None,
    })
}

/// The colours the board is drawn with, taken from the design system.
fn view_style(game: &Game) -> rowline_core::view::ViewStyle {
    let p = &game.design.palette;
    rowline_core::view::ViewStyle {
        background: p.background.into(),
        board_backdrop: p.board_backdrop.into(),
        selection: p.selection.into(),
        path: p.accent.into(),
    }
}

/// The height of the HUD bar, in logical pixels.
const HUD_H: f32 = rowline_core::layout::HUD_H as f32;
/// The original's three-cell preview group is 3 x 36 logical pixels wide.
const PREVIEW_W: f32 = 108.0;
/// The score boxes in the original HUD.
const SCORE_BOX_W: f32 = 80.0;
const SCORE_BOX_H: f32 = 20.0;

/// Draw the HUD bar.
///
/// The original puts a horizontal bar above the board: a score box on the
/// left, the "next group" label and its three preview cells, the colour/shape
/// indicator, and a second score box on the right (docs/11 section 2).
fn hud_commands(game: &Game, layout: Layout) -> Vec<RenderCmd> {
    let mut cmds = Vec::new();
    let ds = &game.design;
    let p = &ds.palette;
    let w = layout.client_w as f32;
    // The play area starts below the drawn menu strip.
    let y = layout.top as f32;
    let text_size = ds.text_size(rowline_theme::TextRole::Body) as u16;
    // The original's HUD captions are the same size as its menu text.
    let label_size = ds.text_size(rowline_theme::TextRole::Body) as u16;

    // The bar itself: the original's system 3D face.
    cmds.push(RenderCmd::RoundedRect {
        rect: rowline_core::render::Rect::new(0.0, y, w, HUD_H),
        radius: 0.0,
        color: p.surface_alt.into(),
    });

    // The readout: a deep plate with warm digits, right-aligned.
    let score_box = |cmds: &mut Vec<RenderCmd>,
                     rect: rowline_core::render::Rect,
                     score: u32,
                     highlight: bool| {
        cmds.push(RenderCmd::RoundedRect {
            rect,
            radius: ds.radius(rowline_theme::Radius::Md),
            color: p.score_fill.into(),
        });
        cmds.push(RenderCmd::Outline {
            rect,
            color: if highlight {
                p.accent.into()
            } else {
                p.score_border.into()
            },
        });
        if highlight {
            // A second, outer accent ring for the new-record flash.
            cmds.push(RenderCmd::Outline {
                rect: rowline_core::render::Rect::new(
                    rect.x - 1.0,
                    rect.y - 1.0,
                    rect.width + 2.0,
                    rect.height + 2.0,
                ),
                color: p.accent.into(),
            });
        }
        let text = format!("{score}");
        let width = 8.0 * text.chars().count() as f32;
        cmds.push(RenderCmd::Text {
            text,
            at: rowline_core::input::Point::new(rect.x + rect.width - 7.0 - width, rect.y + 3.0),
            size: text_size,
            color: p.score_text.into(),
        });
    };

    let box_y = y + (HUD_H - SCORE_BOX_H) * 0.5;
    // Label and box share one row: `纪录 [100]` on the left and
    // `本局 [0]` on the right.  Putting the label above the box made them
    // collide in the 44 px HUD.
    let label_y = y + (HUD_H - 12.0) * 0.5;
    let label = "\u{7eaa}\u{5f55}"; // 纪录
    let left_box = rowline_core::render::Rect::new(64.0, box_y, SCORE_BOX_W, SCORE_BOX_H);
    let right_box =
        rowline_core::render::Rect::new(w - 20.0 - SCORE_BOX_W, box_y, SCORE_BOX_W, SCORE_BOX_H);
    cmds.push(RenderCmd::Text {
        text: label.to_string(),
        at: rowline_core::input::Point::new(20.0, label_y),
        size: ds.text_size(rowline_theme::TextRole::Caption) as u16,
        color: p.text_tertiary.into(),
    });
    score_box(
        &mut cmds,
        left_box,
        game.displayed_king_score(),
        game.record_celebrating(),
    );

    // ---- the next group (three cells, exactly 3 x 36 px wide) ----------
    //
    // 0x40B078: preview group x = (clientW - 0x6C) / 2.
    let group_x = ((w - PREVIEW_W) * 0.5).max(0.0);
    let cell_y = y + (HUD_H - 36.0) * 0.5;
    cmds.push(RenderCmd::Text {
        text: "\u{4e0b}\u{4e00}\u{7ec4}".to_string(),
        at: rowline_core::input::Point::new(group_x - 48.0, y + 12.0),
        size: label_size,
        color: p.text_tertiary.into(),
    });
    for (i, colour) in game.engine().next_group().iter().enumerate() {
        let cx = group_x + i as f32 * 36.0;
        let cell = rowline_core::render::Rect::new(cx, cell_y, 36.0, 36.0);
        // Every slot is a beveled plate, even when it is empty.
        cmds.push(RenderCmd::Sprite {
            sheet: rowline_core::render::SpriteSheetId::MAIN,
            src: rowline_core::view::ball_src(0, game.sheet_scale),
            dst: rowline_core::input::Point::new(cell.x, cell.y),
            size: rowline_core::input::Size::new(36.0, 36.0),
            tint: rowline_core::render::Color::WHITE,
        });
        if *colour != 0 {
            // The preview balls are 24 px, smaller than the board balls.
            ball_at(&mut cmds, game, *colour, cell.x + 18.0, cell.y + 18.0, 24.0);
        }
    }

    // ---- the right score box -------------------------------------------
    let right_label = "\u{672c}\u{5c40}"; // 本局
    cmds.push(RenderCmd::Text {
        text: right_label.to_string(),
        at: rowline_core::input::Point::new(right_box.x - 44.0, label_y),
        size: ds.text_size(rowline_theme::TextRole::Caption) as u16,
        color: p.text_tertiary.into(),
    });
    score_box(&mut cmds, right_box, game.challenger_score(), false);

    cmds
}

// ---------------------------------------------------------------- dialogs
//
// Every modal shares one chrome: a dimmed backdrop, a raised card, a title,
// a close (x) button in the top-right corner and, for informational dialogs,
// a labelled 关闭 button at the bottom.  The geometry helpers are used by
// both the drawing code and the hit-testing, so a button can never be drawn
// in one place and clickable in another.

/// The height of the dialog title strip.
const DIALOG_TITLE_H: f32 = 46.0;
/// The edge of the square close button.
const DIALOG_CLOSE: f32 = 28.0;
/// The footer button.
const DIALOG_BTN_W: f32 = 96.0;
const DIALOG_BTN_H: f32 = 32.0;

/// A centred card, clamped to the client area.
fn dialog_rect(layout: Layout, w: f32, h: f32) -> rowline_core::render::Rect {
    let w = w.min(layout.client_w as f32 - 32.0).max(240.0);
    let h = h.min(layout.client_h as f32 - 32.0).max(160.0);
    rowline_core::render::Rect::new(
        ((layout.client_w as f32 - w) * 0.5).max(8.0),
        ((layout.client_h as f32 - h) * 0.5).max(8.0),
        w,
        h,
    )
}

/// The close (x) button in the title strip.
fn close_rect(panel: rowline_core::render::Rect) -> rowline_core::render::Rect {
    rowline_core::render::Rect::new(
        panel.x + panel.width - DIALOG_CLOSE - 16.0,
        panel.y + 9.0,
        DIALOG_CLOSE,
        DIALOG_CLOSE,
    )
}

/// The labelled footer button.
fn dialog_button_rect(panel: rowline_core::render::Rect) -> rowline_core::render::Rect {
    rowline_core::render::Rect::new(
        panel.x + (panel.width - DIALOG_BTN_W) * 0.5,
        panel.y + panel.height - DIALOG_BTN_H - 16.0,
        DIALOG_BTN_W,
        DIALOG_BTN_H,
    )
}

/// The dimmed layer behind a modal.
fn scrim(game: &Game, layout: Layout) -> RenderCmd {
    RenderCmd::RoundedRect {
        rect: rowline_core::render::Rect::new(
            0.0,
            0.0,
            layout.client_w as f32,
            layout.client_h as f32,
        ),
        radius: 0.0,
        color: game.design.palette.scrim.into(),
    }
}

/// The card, title and close button shared by every dialog.
fn dialog_chrome(game: &Game, panel: rowline_core::render::Rect, title: &str) -> Vec<RenderCmd> {
    let ds = &game.design;
    let p = &ds.palette;
    let mut cmds = vec![
        RenderCmd::Shadow {
            rect: panel,
            radius: ds.radius(rowline_theme::Radius::Lg),
            offset: rowline_core::input::Point::new(0.0, 10.0),
            blur: 36.0,
            color: p.shadow.into(),
        },
        RenderCmd::RoundedRect {
            rect: panel,
            radius: ds.radius(rowline_theme::Radius::Lg),
            color: p.surface_raised.into(),
        },
        RenderCmd::Outline {
            rect: panel,
            color: p.outline.into(),
        },
    ];
    cmds.push(RenderCmd::Text {
        text: title.to_string(),
        at: rowline_core::input::Point::new(panel.x + 20.0, panel.y + 13.0),
        size: ds.text_size(rowline_theme::TextRole::Title) as u16,
        color: p.text_primary.into(),
    });
    // A one-pixel divider under the title strip.
    cmds.push(RenderCmd::RoundedRect {
        rect: rowline_core::render::Rect::new(
            panel.x + 16.0,
            panel.y + DIALOG_TITLE_H - 1.0,
            panel.width - 32.0,
            1.0,
        ),
        radius: 0.0,
        color: p.outline.into(),
    });
    // The close button: a quiet square with an x, familiar from every
    // desktop dialog.  It is the primary way out next to Esc.
    let close = close_rect(panel);
    cmds.push(RenderCmd::RoundedRect {
        rect: close,
        radius: 0.0,
        color: p.surface_alt.into(),
    });
    cmds.push(RenderCmd::Outline {
        rect: close,
        color: p.outline.into(),
    });
    cmds.push(RenderCmd::Text {
        text: "\u{00d7}".to_string(),
        at: rowline_core::input::Point::new(close.x + 9.0, close.y + 3.0),
        size: ds.text_size(rowline_theme::TextRole::Title) as u16,
        color: p.text_primary.into(),
    });
    cmds
}

/// The labelled close button at the bottom of an informational dialog.
fn dialog_button(game: &Game, panel: rowline_core::render::Rect) -> Vec<RenderCmd> {
    let ds = &game.design;
    let p = &ds.palette;
    let btn = dialog_button_rect(panel);
    vec![
        RenderCmd::RoundedRect {
            rect: btn,
            radius: ds.radius(rowline_theme::Radius::Sm),
            color: p.accent.into(),
        },
        RenderCmd::Text {
            text: "\u{5173}\u{95ed}".to_string(),
            at: rowline_core::input::Point::new(btn.x + 32.0, btn.y + 6.0),
            size: ds.text_size(rowline_theme::TextRole::Body) as u16,
            color: p.text_on_accent.into(),
        },
    ]
}

/// Geometry of the statistics dialog: the card and its footer button.
fn stats_rects(layout: Layout) -> (rowline_core::render::Rect, rowline_core::render::Rect) {
    let panel = dialog_rect(layout, 372.0, 392.0);
    (panel, dialog_button_rect(panel))
}

/// Draw the statistics dialog.
///
/// Eight rows: the empty-cell count and the seven ball colours.  The
/// original's tiny docked `XGrdCptDialog` is redesigned as a centred modal
/// so it no longer covers the board, with columns for the count, the share
/// of the board and the number cleared this game.
fn stats_panel(game: &Game, layout: Layout) -> Vec<RenderCmd> {
    let ds = &game.design;
    let p = &ds.palette;
    let (panel, _button) = stats_rects(layout);
    let mut cmds = dialog_chrome(game, panel, "\u{7edf}\u{8ba1}");

    let counts = game.engine().colour_counts();
    let cleared = game.engine().cleared_counts();
    let total = rowline_core::board::BOARD_CELLS as u64;
    let body = ds.text_size(rowline_theme::TextRole::Body) as u16;
    let caption = ds.text_size(rowline_theme::TextRole::Caption) as u16;

    let x_swatch = panel.x + 24.0;
    let x_name = panel.x + 58.0;
    let x_count = panel.x + 168.0;
    let x_share = panel.x + 236.0;
    let x_cleared = panel.x + 300.0;

    let header_y = panel.y + DIALOG_TITLE_H + 8.0;
    for (x, text) in [
        (x_name, "\u{989c}\u{8272}"),
        (x_count, "\u{6570}\u{91cf}"),
        (x_share, "\u{5360}\u{6bd4}"),
        (x_cleared, "\u{5df2}\u{6d88}\u{9664}"),
    ] {
        cmds.push(RenderCmd::Text {
            text: text.to_string(),
            at: rowline_core::input::Point::new(x, header_y),
            size: caption,
            color: p.text_tertiary.into(),
        });
    }

    let row_h = 30.0;
    let first_y = header_y + 24.0;
    for slot in 0..8usize {
        let y = first_y + slot as f32 * row_h;
        if slot.is_multiple_of(2) {
            cmds.push(RenderCmd::RoundedRect {
                rect: rowline_core::render::Rect::new(
                    panel.x + 16.0,
                    y - 4.0,
                    panel.width - 32.0,
                    row_h - 2.0,
                ),
                radius: 0.0,
                color: rowline_core::render::Color::rgb(0xE4, 0xE4, 0xE4),
            });
        }
        if slot == 0 {
            // An empty plate for the empty-cell row.
            cmds.push(RenderCmd::RoundedRect {
                rect: rowline_core::render::Rect::new(x_swatch + 2.0, y - 1.0, 20.0, 20.0),
                radius: 2.0,
                color: p.board_cell.into(),
            });
            cmds.push(RenderCmd::Outline {
                rect: rowline_core::render::Rect::new(x_swatch + 2.0, y - 1.0, 20.0, 20.0),
                color: p.outline_strong.into(),
            });
        } else {
            ball_at(&mut cmds, game, slot as u8, x_swatch + 12.0, y + 8.0, 24.0);
        }
        let name = if slot == 0 { "\u{7a7a}\u{683c}" } else { "" };
        if !name.is_empty() {
            cmds.push(RenderCmd::Text {
                text: name.to_string(),
                at: rowline_core::input::Point::new(x_name, y),
                size: body,
                color: p.text_primary.into(),
            });
        }
        let count = counts[slot];
        let share = count as u64 * 100 / total;
        cmds.push(RenderCmd::Text {
            text: count.to_string(),
            at: rowline_core::input::Point::new(x_count, y),
            size: body,
            color: p.text_primary.into(),
        });
        cmds.push(RenderCmd::Text {
            text: format!("{share}%"),
            at: rowline_core::input::Point::new(x_share, y),
            size: body,
            color: p.text_secondary.into(),
        });
        if slot != 0 {
            cmds.push(RenderCmd::Text {
                text: cleared[slot].to_string(),
                at: rowline_core::input::Point::new(x_cleared, y),
                size: body,
                color: p.text_secondary.into(),
            });
        }
    }

    cmds.extend(dialog_button(game, panel));
    cmds
}

/// Draw the end-of-game dialog, the equivalent of the original's
/// `游戏结束!` message box (docs/11 section 5).
fn game_over_dialog(game: &Game, layout: Layout) -> Vec<RenderCmd> {
    let mut cmds = Vec::new();
    let ds = &game.design;
    let p = &ds.palette;

    let w = 320.0_f32.min(layout.client_w as f32 - 32.0).max(200.0);
    let h = 168.0;
    let rect = rowline_core::render::Rect::new(
        ((layout.client_w as f32 - w) * 0.5).max(8.0),
        ((layout.client_h as f32 - h) * 0.5).max(8.0),
        w,
        h,
    );
    cmds.extend(dialog_chrome(
        game,
        rect,
        "\u{6e38}\u{620f}\u{7ed3}\u{675f}",
    ));

    // The warning mark: the original's yellow warning triangle reduced to a
    // trio of stacked bars (a diamond is not available as a primitive).
    let mark = rowline_core::render::Rect::new(rect.x + 24.0, rect.y + 58.0, 40.0, 40.0);
    let yellow = rowline_core::render::Color::rgb(255, 210, 45);
    let dark = rowline_core::render::Color::rgb(32, 32, 32);
    cmds.push(RenderCmd::RoundedRect {
        rect: mark,
        radius: 4.0,
        color: yellow,
    });
    cmds.push(RenderCmd::Text {
        text: "!".to_string(),
        at: rowline_core::input::Point::new(mark.x + 15.0, mark.y + 6.0),
        size: ds.text_size(rowline_theme::TextRole::Title) as u16,
        color: dark,
    });

    cmds.push(RenderCmd::Text {
        text: "\u{65e0}\u{6cd5}\u{79fb}\u{52a8}\u{4e86}".to_string(),
        at: rowline_core::input::Point::new(rect.x + 80.0, rect.y + 58.0),
        size: ds.text_size(rowline_theme::TextRole::Title) as u16,
        color: p.text_primary.into(),
    });
    cmds.push(RenderCmd::Text {
        text: "\u{67e5}\u{770b}\u{9ad8}\u{5206}\u{699c}\u{ff0c}\u{6216}\u{5f00}\u{59cb}\u{65b0}\u{6e38}\u{620f}".to_string(),
        at: rowline_core::input::Point::new(rect.x + 80.0, rect.y + 86.0),
        size: ds.text_size(rowline_theme::TextRole::Caption) as u16,
        color: p.text_secondary.into(),
    });

    // The OK button.
    let btn = rowline_core::render::Rect::new(
        rect.x + (rect.width - 120.0) * 0.5,
        rect.y + rect.height - 52.0,
        120.0,
        36.0,
    );
    cmds.push(RenderCmd::RoundedRect {
        rect: btn,
        radius: ds.radius(rowline_theme::Radius::Sm),
        color: p.accent.into(),
    });
    cmds.push(RenderCmd::Text {
        text: "\u{786e}\u{5b9a}".to_string(),
        at: rowline_core::input::Point::new(btn.x + 48.0, btn.y + 9.0),
        size: ds.text_size(rowline_theme::TextRole::Body) as u16,
        color: p.text_on_accent.into(),
    });
    cmds
}

/// Geometry of the about dialog: the card and its footer button.
fn about_rects(layout: Layout) -> (rowline_core::render::Rect, rowline_core::render::Rect) {
    let panel = dialog_rect(layout, 430.0, 300.0);
    (panel, dialog_button_rect(panel))
}

/// Draw the about dialog.
fn about_overlay(game: &Game, layout: Layout) -> Vec<RenderCmd> {
    let ds = &game.design;
    let p = &ds.palette;
    let (panel, _button) = about_rects(layout);
    let mut cmds = dialog_chrome(game, panel, "\u{5173}\u{4e8e} Rowline");

    let body = ds.text_size(rowline_theme::TextRole::Body) as u16;
    let small = ds.text_size(rowline_theme::TextRole::Caption) as u16;
    let lines = [
        (
            format!(
                "{} {}",
                rowline_core::PRODUCT_NAME,
                env!("CARGO_PKG_VERSION")
            ),
            body,
            p.text_primary,
        ),
        (
            "WinLinez 行为的独立重实现".to_string(),
            body,
            p.text_secondary,
        ),
        (
            "不含原作的代码、美术或数据".to_string(),
            body,
            p.text_secondary,
        ),
        ("Rust · winit · wgpu".to_string(), small, p.text_tertiary),
        (
            "界面字体：WenQuanYi Micro Hei (Apache-2.0)".to_string(),
            small,
            p.text_tertiary,
        ),
        (
            "原作 WinLinez © 1998-1999 Ivan Golubev".to_string(),
            small,
            p.text_tertiary,
        ),
    ];
    for (i, (line, size, color)) in lines.iter().enumerate() {
        cmds.push(RenderCmd::Text {
            text: line.clone(),
            at: rowline_core::input::Point::new(panel.x + 24.0, panel.y + 68.0 + i as f32 * 26.0),
            size: *size,
            color: (*color).into(),
        });
    }

    cmds.extend(dialog_button(game, panel));
    cmds
}

/// Draw the two figures that flank the board: the CPU (国王) on the left and
/// the player (挑战者) on the right.
///
/// The original draws two animated kings on metal pedestals, taken from the
/// character strip of its sprite atlas.  That artwork belongs to the
/// original author, so it is **not** reused here (REPORT.md rule R2): the
/// figures are drawn from the theme's own primitives instead -- a crowned
/// king for the CPU and a sword-bearing challenger for the human, both
/// standing on a metallic pedestal.
fn figure_commands(game: &Game, layout: Layout) -> Vec<RenderCmd> {
    let mut cmds = Vec::new();
    // Both figures stand on the same floor; their height above it encodes
    // their score.  The king represents the record, the challenger the
    // current round, so the king starts higher and the challenger overtakes
    // him as the round beats the record.
    let floor = layout.figure_y as f32 + FIGURE_FLOOR;
    cmds.extend(one_figure(
        game,
        layout,
        FigureSpec {
            slot_x: layout.left_figure_x,
            label: rowline_core::scoreboard::KING_NAME,
            figure: Figure::King,
            feet: floor - FIGURE_STEP - game.king_rise,
            lift: 0.0,
            phase: 0.0,
        },
    ));
    cmds.extend(one_figure(
        game,
        layout,
        FigureSpec {
            slot_x: layout.right_figure_x,
            label: rowline_core::scoreboard::CHALLENGER_NAME,
            figure: Figure::Challenger,
            feet: floor - FIGURE_STEP - game.challenger_rise,
            lift: win_lift(game),
            // A different phase, so the two figures do not blink in sync.
            phase: 1.7,
        },
    ));
    cmds
}

/// Everything that places one figure.
#[derive(Clone, Copy)]
struct FigureSpec {
    /// The left edge of the figure's slot, in logical pixels.
    slot_x: i32,
    /// The caption under the pedestal.
    label: &'static str,
    /// Which character to draw.
    figure: Figure,
    /// Where the character's feet are, in window coordinates.
    feet: f32,
    /// The victory hop, in logical pixels.
    lift: f32,
    /// The idle-animation phase.
    phase: f32,
}

/// How far the winner's medallion is lifted during the victory animation.
fn win_lift(game: &Game) -> f32 {
    let Some(started) = game.win_anim else {
        return 0.0;
    };
    let elapsed = started.elapsed().as_secs_f32();
    let total = WIN_ANIM.as_secs_f32();
    if elapsed >= total {
        return 0.0;
    }
    let t = elapsed / total;
    -(t * std::f32::consts::TAU).sin().abs() * 6.0
}

/// Which figure is drawn.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Figure {
    /// The CPU: a king with a crown.
    King,
    /// The human: a challenger with a sword.
    Challenger,
}

/// The width of the pedestal column, in logical pixels.
const PEDESTAL_W: f32 = 30.0;
/// The width of the pedestal's base plate, in logical pixels.
const PEDESTAL_FOOT_W: f32 = 48.0;
/// The height of the pedestal's base plate, in logical pixels.
const PEDESTAL_FOOT_H: f32 = 10.0;
/// The height of the original's figure slot (portrait plus pedestal).
const FIGURE_SLOT_H: f32 = 134.0;
/// The floor both figures stand on, relative to `Layout::figure_y`: the
/// pedestal's base plate.  Their height above it encodes their score.
const FIGURE_FLOOR: f32 = FIGURE_SLOT_H;
/// A small constant pedestal, so a figure at score 0 still stands on
/// something instead of sitting on its caption.
const FIGURE_STEP: f32 = 10.0;
/// The label's glyph size, matching the original's large captions.
const FIGURE_LABEL_SIZE: u16 = 26;

/// Draw one cartoon figure and its pedestal.
///
/// The original's kings are its own artwork, so the re-implementation draws
/// a **new** cartoon character from theme primitives: a round body, a face
/// with eyes and a smile, a golden crown, and a sceptre (CPU) or a sword
/// (challenger), standing on a metal pedestal.  The layout -- feet, pedestal,
/// caption -- follows the measured original geometry.
fn one_figure(game: &Game, layout: Layout, spec: FigureSpec) -> Vec<RenderCmd> {
    let FigureSpec {
        slot_x,
        label,
        figure,
        feet,
        lift,
        phase,
    } = spec;
    let ds = &game.design;
    let p = &ds.palette;
    let mut cmds = Vec::new();

    let centre = slot_x as f32 + rowline_core::layout::FIGURE_W as f32 / 2.0;
    let pedestal_bottom = layout.figure_y as f32 + FIGURE_FLOOR;

    // ---- idle animation: a slow bob, plus the victory hop ---------------
    let idle = (game.clock_seconds() * 2.4 + phase).sin() * 1.2;
    let feet = feet + idle + lift;

    // ---- the pedestal ---------------------------------------------------
    //
    // A metal column lit from the upper left: a soft highlight down the left
    // edge, a darker shade on the right, two machined rings, a beveled cap
    // and base, a cast shadow and a pool of light on the floor.
    let column = rowline_core::render::Rect::new(
        centre - PEDESTAL_W / 2.0,
        feet + 1.0,
        PEDESTAL_W,
        (pedestal_bottom - 4.0 - feet - 1.0).max(4.0),
    );
    let foot = rowline_core::render::Rect::new(
        centre - PEDESTAL_FOOT_W / 2.0,
        pedestal_bottom - PEDESTAL_FOOT_H,
        PEDESTAL_FOOT_W,
        PEDESTAL_FOOT_H,
    );
    let cap = rowline_core::render::Rect::new(
        centre - PEDESTAL_W / 2.0 - 2.0,
        feet - 3.0,
        PEDESTAL_W + 4.0,
        6.0,
    );
    let white_soft = rowline_core::render::Color {
        r: 255,
        g: 255,
        b: 255,
        a: 70,
    };
    let shade = rowline_core::render::Color {
        r: 0,
        g: 0,
        b: 0,
        a: 55,
    };
    let steel_light = rowline_core::render::Color::rgb(248, 248, 248);
    let steel_mid = rowline_core::render::Color::rgb(196, 196, 196);
    let steel_dark = rowline_core::render::Color::rgb(132, 132, 132);

    // A reflection under the base and a tight contact shadow: the pedestal
    // sits on the black floor without a grey halo around it.
    cmds.push(RenderCmd::Gradient {
        rect: rowline_core::render::Rect::new(
            foot.x - 10.0,
            foot.y + foot.height - 6.0,
            foot.width + 20.0,
            18.0,
        ),
        radius: 9.0,
        from: rowline_core::render::Color {
            r: 255,
            g: 255,
            b: 255,
            a: 42,
        },
        to: rowline_core::render::Color {
            r: 255,
            g: 255,
            b: 255,
            a: 0,
        },
        direction: rowline_core::render::GradientDirection::Vertical,
    });
    cmds.push(RenderCmd::RoundedRect {
        rect: rowline_core::render::Rect::new(
            foot.x + 5.0,
            foot.y + foot.height - 3.0,
            foot.width - 10.0,
            4.0,
        ),
        radius: 2.0,
        color: rowline_core::render::Color {
            r: 0,
            g: 0,
            b: 0,
            a: 150,
        },
    });

    // Column body: lit at the left, shaded at the right.
    cmds.push(RenderCmd::Gradient {
        rect: column,
        radius: 3.0,
        from: steel_light,
        to: steel_dark,
        direction: rowline_core::render::GradientDirection::Horizontal,
    });
    cmds.push(RenderCmd::RoundedRect {
        rect: rowline_core::render::Rect::new(
            column.x + 3.0,
            column.y + 2.0,
            3.5,
            (column.height - 4.0).max(2.0),
        ),
        radius: 1.75,
        color: white_soft,
    });
    cmds.push(RenderCmd::RoundedRect {
        rect: rowline_core::render::Rect::new(
            column.x + column.width - 7.0,
            column.y + 2.0,
            4.0,
            (column.height - 4.0).max(2.0),
        ),
        radius: 2.0,
        color: shade,
    });
    // Two machined rings break up the tall column.
    for t in [0.32_f32, 0.66] {
        let y = column.y + column.height * t;
        cmds.push(RenderCmd::RoundedRect {
            rect: rowline_core::render::Rect::new(column.x, y, column.width, 1.0),
            radius: 0.0,
            color: rowline_core::render::Color::rgb(160, 160, 160),
        });
        cmds.push(RenderCmd::RoundedRect {
            rect: rowline_core::render::Rect::new(column.x, y + 1.0, column.width, 1.0),
            radius: 0.0,
            color: white_soft,
        });
    }
    cmds.push(RenderCmd::Outline {
        rect: column,
        color: p.outline.into(),
    });

    // The cap under the figure's feet, lit from above.
    cmds.push(RenderCmd::Gradient {
        rect: cap,
        radius: 3.0,
        from: steel_light,
        to: steel_mid,
        direction: rowline_core::render::GradientDirection::Vertical,
    });
    cmds.push(RenderCmd::Outline {
        rect: cap,
        color: p.outline.into(),
    });
    // The base plate: a bright top edge, a shaded lower half, and a rim.
    cmds.push(RenderCmd::Gradient {
        rect: foot,
        radius: 3.0,
        from: steel_light,
        to: steel_mid,
        direction: rowline_core::render::GradientDirection::Vertical,
    });
    cmds.push(RenderCmd::RoundedRect {
        rect: rowline_core::render::Rect::new(foot.x + 2.0, foot.y + 1.0, foot.width - 4.0, 2.0),
        radius: 1.0,
        color: white_soft,
    });
    cmds.push(RenderCmd::RoundedRect {
        rect: rowline_core::render::Rect::new(
            foot.x + 2.0,
            foot.y + foot.height - 3.0,
            foot.width - 4.0,
            2.0,
        ),
        radius: 1.0,
        color: shade,
    });
    cmds.push(RenderCmd::Outline {
        rect: foot,
        color: p.outline.into(),
    });

    // ---- the character --------------------------------------------------
    // Saturated bodies with a dark rim: on the black board a pale pastel
    // body washed out, and the rim keeps the silhouette readable.  A soft
    // coloured aura separates the figure from the board.
    let (body_light, body_dark, belly_colour, rim_colour, aura) = match figure {
        Figure::King => (
            rowline_core::render::Color::rgb(214, 66, 66),
            rowline_core::render::Color::rgb(138, 8, 8),
            rowline_core::render::Color::rgb(238, 128, 128),
            rowline_core::render::Color::rgb(82, 0, 0),
            rowline_core::render::Color {
                r: 240,
                g: 80,
                b: 60,
                a: 34,
            },
        ),
        Figure::Challenger => (
            rowline_core::render::Color::rgb(204, 64, 204),
            rowline_core::render::Color::rgb(104, 0, 104),
            rowline_core::render::Color::rgb(238, 140, 238),
            rowline_core::render::Color::rgb(72, 0, 72),
            rowline_core::render::Color {
                r: 220,
                g: 80,
                b: 220,
                a: 30,
            },
        ),
    };
    let gold = rowline_core::render::Color::rgb(255, 210, 74);
    let gold_dark = rowline_core::render::Color::rgb(176, 118, 12);
    let dark = rowline_core::render::Color::rgb(48, 16, 16);
    let white = rowline_core::render::Color::WHITE;

    // A coloured aura behind the head and body, visible against the black
    // board without washing out the silhouette.
    cmds.push(RenderCmd::Glow {
        rect: rowline_core::render::Rect::new(centre - 32.0, feet - 80.0, 64.0, 84.0),
        radius: 30.0,
        blur: 26.0,
        color: aura,
    });

    // The king's cape sits behind the body; the challenger goes without one.
    if figure == Figure::King {
        cmds.push(RenderCmd::RoundedRect {
            rect: rowline_core::render::Rect::new(centre - 28.0, feet - 40.0, 56.0, 32.0),
            radius: 14.0,
            color: rowline_core::render::Color::rgb(96, 0, 0),
        });
        cmds.push(RenderCmd::RoundedRect {
            rect: rowline_core::render::Rect::new(centre - 26.0, feet - 38.0, 52.0, 9.0),
            radius: 4.5,
            color: rowline_core::render::Color::rgb(190, 24, 24),
        });
    }

    // Arms first, so the body overlaps their inner ends.
    for dx in [-24.0_f32, 17.0] {
        cmds.push(RenderCmd::RoundedRect {
            rect: rowline_core::render::Rect::new(centre + dx, feet - 34.0, 9.0, 18.0),
            radius: 4.5,
            color: body_dark,
        });
        cmds.push(RenderCmd::RoundedRect {
            rect: rowline_core::render::Rect::new(centre + dx + 1.0, feet - 33.0, 3.0, 14.0),
            radius: 1.5,
            color: white_soft,
        });
    }
    // Body, with a rounded rim drawn as a slightly larger disc behind it
    // (`Outline` would be a square frame around the cocoon).
    let body_rect = rowline_core::render::Rect::new(centre - 22.0, feet - 38.0, 44.0, 38.0);
    cmds.push(RenderCmd::RoundedRect {
        rect: rowline_core::render::Rect::new(
            body_rect.x - 1.5,
            body_rect.y - 1.5,
            body_rect.width + 3.0,
            body_rect.height + 3.0,
        ),
        radius: 19.5,
        color: rim_colour,
    });
    cmds.push(RenderCmd::Gradient {
        rect: body_rect,
        radius: 18.0,
        from: body_light,
        to: body_dark,
        direction: rowline_core::render::GradientDirection::Horizontal,
    });
    // A soft specular highlight on the upper left of the body.
    cmds.push(RenderCmd::RoundedRect {
        rect: rowline_core::render::Rect::new(centre - 15.0, feet - 34.0, 9.0, 12.0),
        radius: 4.5,
        color: white_soft,
    });
    // Belly.
    cmds.push(RenderCmd::RoundedRect {
        rect: rowline_core::render::Rect::new(centre - 13.0, feet - 29.0, 26.0, 22.0),
        radius: 12.0,
        color: belly_colour,
    });
    match figure {
        Figure::King => {
            // A gold medallion on the chest.
            cmds.push(RenderCmd::RoundedRect {
                rect: rowline_core::render::Rect::new(centre - 3.5, feet - 25.0, 7.0, 7.0),
                radius: 3.5,
                color: gold,
            });
            cmds.push(RenderCmd::RoundedRect {
                rect: rowline_core::render::Rect::new(centre - 1.5, feet - 23.0, 3.0, 3.0),
                radius: 1.5,
                color: rowline_core::render::Color::rgb(220, 32, 48),
            });
        }
        Figure::Challenger => {
            // A leather belt with a steel buckle.
            cmds.push(RenderCmd::RoundedRect {
                rect: rowline_core::render::Rect::new(centre - 16.0, feet - 18.0, 32.0, 4.0),
                radius: 2.0,
                color: rowline_core::render::Color::rgb(58, 0, 58),
            });
            cmds.push(RenderCmd::RoundedRect {
                rect: rowline_core::render::Rect::new(centre - 3.0, feet - 19.0, 6.0, 6.0),
                radius: 1.5,
                color: rowline_core::render::Color::rgb(214, 214, 214),
            });
        }
    }
    // Head.
    let head_y = feet - 38.0 - 26.0;
    let head_rect = rowline_core::render::Rect::new(centre - 16.0, head_y, 32.0, 30.0);
    cmds.push(RenderCmd::RoundedRect {
        rect: rowline_core::render::Rect::new(
            head_rect.x - 1.5,
            head_rect.y - 1.5,
            head_rect.width + 3.0,
            head_rect.height + 3.0,
        ),
        radius: 16.5,
        color: rim_colour,
    });
    cmds.push(RenderCmd::Gradient {
        rect: head_rect,
        radius: 15.0,
        from: body_light,
        to: body_dark,
        direction: rowline_core::render::GradientDirection::Horizontal,
    });
    cmds.push(RenderCmd::RoundedRect {
        rect: rowline_core::render::Rect::new(centre - 12.0, head_y + 2.0, 6.0, 5.0),
        radius: 3.0,
        color: white_soft,
    });
    // Face patch.
    cmds.push(RenderCmd::RoundedRect {
        rect: rowline_core::render::Rect::new(centre - 12.0, head_y + 8.0, 24.0, 18.0),
        radius: 9.0,
        color: belly_colour,
    });
    // Eyes.  They blink for a tenth of a second every few seconds.
    let blink = (game.clock_seconds() + phase) % 3.7 < 0.12;
    for dx in [-8.0_f32, 3.0] {
        if blink {
            cmds.push(RenderCmd::RoundedRect {
                rect: rowline_core::render::Rect::new(centre + dx, head_y + 16.0, 5.0, 1.5),
                radius: 0.75,
                color: dark,
            });
            continue;
        }
        cmds.push(RenderCmd::RoundedRect {
            rect: rowline_core::render::Rect::new(centre + dx, head_y + 11.0, 5.0, 6.0),
            radius: 2.5,
            color: white,
        });
        cmds.push(RenderCmd::RoundedRect {
            rect: rowline_core::render::Rect::new(centre + dx + 1.5, head_y + 13.0, 2.5, 3.0),
            radius: 1.25,
            color: dark,
        });
    }
    // Smile.
    cmds.push(RenderCmd::RoundedRect {
        rect: rowline_core::render::Rect::new(centre - 5.0, head_y + 21.0, 10.0, 2.0),
        radius: 1.0,
        color: dark,
    });

    // ---- headwear -------------------------------------------------------
    //
    // Only the king wears a crown.  The challenger gets a steel headband
    // with a knot and two trailing ends, which reads as a fighter rather
    // than a second monarch.
    match figure {
        Figure::King => {
            let band = rowline_core::render::Rect::new(centre - 14.0, head_y - 5.0, 28.0, 8.0);
            cmds.push(RenderCmd::Glow {
                rect: rowline_core::render::Rect::new(
                    band.x - 5.0,
                    band.y - 12.0,
                    band.width + 10.0,
                    band.height + 20.0,
                ),
                radius: 8.0,
                blur: 14.0,
                color: rowline_core::render::Color {
                    r: 255,
                    g: 210,
                    b: 74,
                    a: 55,
                },
            });
            cmds.push(RenderCmd::Gradient {
                rect: band,
                radius: 2.5,
                from: gold,
                to: gold_dark,
                direction: rowline_core::render::GradientDirection::Vertical,
            });
            cmds.push(RenderCmd::Outline {
                rect: band,
                color: gold_dark,
            });
            for (dx, h) in [(-10.0_f32, 8.0_f32), (-2.0, 12.0), (6.0, 8.0)] {
                let point =
                    rowline_core::render::Rect::new(centre + dx, head_y - 5.0 - h, 4.5, h + 2.0);
                cmds.push(RenderCmd::RoundedRect {
                    rect: point,
                    radius: 1.5,
                    color: gold,
                });
                cmds.push(RenderCmd::RoundedRect {
                    rect: rowline_core::render::Rect::new(point.x + 1.0, point.y, 1.5, 2.5),
                    radius: 0.75,
                    color: white,
                });
            }
            // Three jewels on the band.
            for (dx, colour) in [
                (-8.0_f32, rowline_core::render::Color::rgb(220, 32, 48)),
                (0.0, rowline_core::render::Color::rgb(64, 132, 255)),
                (8.0, rowline_core::render::Color::rgb(48, 176, 96)),
            ] {
                cmds.push(RenderCmd::RoundedRect {
                    rect: rowline_core::render::Rect::new(
                        centre + dx - 1.5,
                        head_y - 2.5,
                        3.0,
                        3.0,
                    ),
                    radius: 1.5,
                    color: colour,
                });
            }
        }
        Figure::Challenger => {
            // A short, tousled fringe instead of a crown.
            for (dx, w, h) in [
                (-11.0_f32, 8.0_f32, 8.0_f32),
                (-2.0, 7.0, 11.0),
                (6.0, 8.0, 8.0),
            ] {
                cmds.push(RenderCmd::RoundedRect {
                    rect: rowline_core::render::Rect::new(centre + dx, head_y - 4.0, w, h),
                    radius: w * 0.5,
                    color: body_dark,
                });
            }
        }
    }

    // ---- the emblem: sceptre for the CPU, sword for the challenger ------
    match figure {
        Figure::King => {
            // A sceptre held in the right hand: a shaded shaft, an orb with
            // a highlight, and a grip in the body colour.
            let shaft = rowline_core::render::Rect::new(centre + 22.0, feet - 60.0, 4.0, 36.0);
            cmds.push(RenderCmd::Gradient {
                rect: shaft,
                radius: 2.0,
                from: gold,
                to: gold_dark,
                direction: rowline_core::render::GradientDirection::Horizontal,
            });
            cmds.push(RenderCmd::Glow {
                rect: rowline_core::render::Rect::new(centre + 15.0, feet - 74.0, 18.0, 18.0),
                radius: 9.0,
                blur: 12.0,
                color: rowline_core::render::Color {
                    r: 255,
                    g: 210,
                    b: 74,
                    a: 70,
                },
            });
            cmds.push(RenderCmd::RoundedRect {
                rect: rowline_core::render::Rect::new(centre + 18.0, feet - 70.0, 12.0, 12.0),
                radius: 6.0,
                color: gold,
            });
            cmds.push(RenderCmd::RoundedRect {
                rect: rowline_core::render::Rect::new(centre + 21.0, feet - 68.0, 4.0, 4.0),
                radius: 2.0,
                color: white,
            });
            cmds.push(RenderCmd::RoundedRect {
                rect: rowline_core::render::Rect::new(centre + 19.0, feet - 40.0, 9.0, 8.0),
                radius: 4.0,
                color: body_dark,
            });
        }
        Figure::Challenger => {
            cmds.push(RenderCmd::Glow {
                rect: rowline_core::render::Rect::new(centre + 14.0, feet - 74.0, 30.0, 46.0),
                radius: 10.0,
                blur: 12.0,
                color: rowline_core::render::Color {
                    r: 240,
                    g: 240,
                    b: 240,
                    a: 34,
                },
            });
            cmds.push(RenderCmd::Icon {
                index: crate::commands::ICON_SWORD,
                dst: rowline_core::input::Point::new(centre + 18.0, feet - 70.0),
                size: rowline_core::input::Size::new(24.0, 36.0),
                tint: rowline_core::render::Color::rgb(240, 240, 240),
            });
            // The gripping hand and a steel bracer.
            cmds.push(RenderCmd::RoundedRect {
                rect: rowline_core::render::Rect::new(centre + 20.0, feet - 39.0, 9.0, 8.0),
                radius: 4.0,
                color: body_dark,
            });
            cmds.push(RenderCmd::RoundedRect {
                rect: rowline_core::render::Rect::new(centre + 17.0, feet - 33.0, 10.0, 5.0),
                radius: 2.5,
                color: rowline_core::render::Color::rgb(200, 200, 200),
            });
        }
    }

    // ---- the caption ----------------------------------------------------
    let label_w = label.chars().count() as f32 * FIGURE_LABEL_SIZE as f32 * 0.62;
    cmds.push(RenderCmd::Text {
        text: label.to_string(),
        at: rowline_core::input::Point::new(
            centre - label_w * 0.5,
            // The caption sits below the floor, so a figure standing on the
            // floor (score 0) does not cover it.
            layout.figure_y as f32 + FIGURE_FLOOR + 6.0,
        ),
        size: FIGURE_LABEL_SIZE,
        color: p.text_on_accent.into(),
    });

    cmds
}

/// The leaderboard geometry, shared by drawing and hit-testing.
fn scoreboard_rects(layout: Layout) -> (rowline_core::render::Rect, f32) {
    // The original's leaderboard is a separate dialog that can be taller
    // than the game window.  The re-implementation draws it as an overlay,
    // so the row height shrinks to keep all sixteen rows on screen at the
    // original's small default client size.
    let content_h = (layout.client_h - layout.top).max(1) as f32;
    let margin = 4.0;
    let w = 420.0_f32.min(layout.client_w as f32 - 32.0).max(240.0);
    // The panel fills the play area below the HUD, so the preview cells are
    // not left half-covered above it.  The title strip (46 px) and the
    // button footer (56 px) are reserved before the sixteen rows share the
    // rest.
    let h = (content_h - 2.0 * margin).max(120.0);
    let row_h = ((h - DIALOG_TITLE_H - 56.0) / 16.0).max(8.0);
    let rect = rowline_core::render::Rect::new(
        ((layout.client_w as f32 - w) * 0.5).max(margin),
        layout.top as f32 + margin,
        w,
        h,
    );
    (rect, row_h)
}

/// The two buttons of the leaderboard, laid out as a centred pair that stays
/// inside the panel.
fn scoreboard_buttons(
    (rect, _row_h): (rowline_core::render::Rect, f32),
) -> (rowline_core::render::Rect, rowline_core::render::Rect) {
    const BTN_W: f32 = 120.0;
    const BTN_H: f32 = 30.0;
    const GAP: f32 = 12.0;
    let total = BTN_W * 2.0 + GAP;
    let x = rect.x + (rect.width - total) * 0.5;
    let y = rect.y + rect.height - BTN_H - 10.0;
    (
        rowline_core::render::Rect::new(x, y, BTN_W, BTN_H),
        rowline_core::render::Rect::new(x + BTN_W + GAP, y, BTN_W, BTN_H),
    )
}

/// The name-entry geometry, shared by drawing and hit-testing.
fn name_entry_rects(
    layout: Layout,
) -> (
    rowline_core::render::Rect,
    rowline_core::render::Rect,
    rowline_core::render::Rect,
) {
    let w = 400.0_f32.min(layout.client_w as f32 - 32.0).max(240.0);
    let h = 210.0;
    let rect = rowline_core::render::Rect::new(
        ((layout.client_w as f32 - w) * 0.5).max(8.0),
        ((layout.client_h as f32 - h) * 0.5).max(8.0),
        w,
        h,
    );
    let field = rowline_core::render::Rect::new(
        rect.x + 20.0,
        rect.y + DIALOG_TITLE_H + 30.0,
        rect.width - 40.0,
        34.0,
    );
    let ok = rowline_core::render::Rect::new(
        rect.x + (rect.width - 120.0) * 0.5,
        rect.y + rect.height - 52.0,
        120.0,
        34.0,
    );
    (rect, field, ok)
}

/// Draw the leaderboard dialog (总榜).
fn scoreboard_overlay(game: &Game, layout: Layout) -> Vec<RenderCmd> {
    let mut cmds = Vec::new();
    let ds = &game.design;
    let p = &ds.palette;
    let sb = rowline_core::scoreboard::Scoreboard::new(game.scores().to_vec());

    let (rect, row_h) = scoreboard_rects(layout);
    cmds.extend(dialog_chrome(game, rect, "\u{603b}\u{699c}"));

    // The sixteen rows.  The rank/name column starts at a fixed x and the
    // score column is right-aligned, so mixed-width names (Latin + CJK)
    // cannot push the numbers out of line.
    let top = rect.y + DIALOG_TITLE_H + 4.0;
    let row_size = ds.text_size(rowline_theme::TextRole::Body) as u16;
    let alt = rowline_core::render::Color::rgb(0xE4, 0xE4, 0xE4);
    for (i, row) in sb.rows().iter().enumerate() {
        let y = top + i as f32 * row_h;
        if i % 2 == 1 {
            cmds.push(RenderCmd::RoundedRect {
                rect: rowline_core::render::Rect::new(
                    rect.x + 8.0,
                    y - 1.0,
                    rect.width - 16.0,
                    row_h,
                ),
                radius: 0.0,
                color: alt,
            });
        }
        let colour = if row.empty {
            p.text_tertiary.into()
        } else {
            p.text_primary.into()
        };
        cmds.push(RenderCmd::Text {
            text: format!("{}. {}", row.rank, row.name),
            at: rowline_core::input::Point::new(rect.x + 16.0, y),
            size: row_size,
            color: colour,
        });
        let score = row.score.to_string();
        // Digits at this size are about eight logical pixels wide.
        let score_w = 8.0 * score.chars().count() as f32;
        cmds.push(RenderCmd::Text {
            text: score,
            at: rowline_core::input::Point::new(rect.x + rect.width - 20.0 - score_w, y),
            size: row_size,
            color: colour,
        });
    }

    // The two buttons: Clear (quiet, danger text) and OK (primary accent).
    let (clear, ok) = scoreboard_buttons((rect, row_h));
    let body = ds.text_size(rowline_theme::TextRole::Body) as u16;
    for (btn, label, filled, colour) in [
        (clear, "\u{6e05}\u{9664}", false, p.danger),
        (ok, "\u{786e}\u{5b9a}", true, p.text_on_accent),
    ] {
        cmds.push(RenderCmd::RoundedRect {
            rect: btn,
            radius: ds.radius(rowline_theme::Radius::Sm),
            color: if filled {
                p.accent.into()
            } else {
                p.surface_alt.into()
            },
        });
        if !filled {
            cmds.push(RenderCmd::Outline {
                rect: btn,
                color: p.outline.into(),
            });
        }
        let text_w = 12.0 * label.chars().count() as f32;
        cmds.push(RenderCmd::Text {
            text: label.to_string(),
            at: rowline_core::input::Point::new(btn.x + (btn.width - text_w) * 0.5, btn.y + 6.0),
            size: body,
            color: colour.into(),
        });
    }
    cmds
}

/// A cheap width estimate for the caret and the selection highlight.
///
/// CJK glyphs are one em wide; Latin glyphs are roughly 0.58 em in the
/// embedded face.  Exact metrics are not needed for a caret.
fn estimate_text_width(text: &str, size: f32) -> f32 {
    text.chars()
        .map(|c| if c.is_ascii() { size * 0.58 } else { size })
        .sum()
}

/// Draw the name entry dialog (输入你的名字).
fn name_entry_overlay(game: &Game, layout: Layout) -> Vec<RenderCmd> {
    let mut cmds = Vec::new();
    let ds = &game.design;
    let p = &ds.palette;

    let (rect, field, ok) = name_entry_rects(layout);
    cmds.extend(dialog_chrome(
        game,
        rect,
        "\u{8f93}\u{5165}\u{4f60}\u{7684}\u{540d}\u{5b57}",
    ));

    cmds.push(RenderCmd::Text {
        text: "\u{4f60}\u{7684}\u{540d}\u{5b57}\u{5c06}\u{8bb0}\u{5165}\u{9ad8}\u{5206}\u{699c}"
            .to_string(),
        at: rowline_core::input::Point::new(rect.x + 20.0, rect.y + DIALOG_TITLE_H + 6.0),
        size: ds.text_size(rowline_theme::TextRole::Caption) as u16,
        color: p.text_tertiary.into(),
    });

    let (field_fill, field_text) = (
        rowline_core::render::Color::WHITE,
        rowline_core::render::Color::BLACK,
    );
    cmds.push(RenderCmd::RoundedRect {
        rect: field,
        radius: ds.radius(rowline_theme::Radius::Sm),
        color: field_fill,
    });
    cmds.push(RenderCmd::Outline {
        rect: field,
        color: p.outline_strong.into(),
    });
    let body_size = ds.text_size(rowline_theme::TextRole::Body) as u16;
    let text_x = field.x + 8.0;
    let text_width = estimate_text_width(&game.name_input, body_size as f32);
    if game.name_select_all {
        // The default name is selected: show a highlight so the user can
        // replace or delete it with one keystroke.
        cmds.push(RenderCmd::RoundedRect {
            rect: rowline_core::render::Rect::new(
                field.x + 4.0,
                field.y + 5.0,
                (text_width + 8.0).min(field.width - 8.0),
                24.0,
            ),
            radius: 0.0,
            color: p.accent.into(),
        });
    }
    cmds.push(RenderCmd::Text {
        text: game.name_input.clone(),
        at: rowline_core::input::Point::new(text_x, field.y + 7.0),
        size: body_size,
        color: if game.name_select_all {
            p.text_on_accent.into()
        } else {
            field_text
        },
    });
    if !game.name_select_all {
        // A caret at the insertion point (text always appends at the end).
        let caret_x = (text_x + text_width + 1.0).min(field.x + field.width - 4.0);
        cmds.push(RenderCmd::RoundedRect {
            rect: rowline_core::render::Rect::new(caret_x, field.y + 8.0, 1.5, 18.0),
            radius: 0.0,
            color: field_text,
        });
    }

    // The primary action.
    cmds.push(RenderCmd::RoundedRect {
        rect: ok,
        radius: ds.radius(rowline_theme::Radius::Sm),
        color: p.accent.into(),
    });
    cmds.push(RenderCmd::Text {
        text: "\u{786e}\u{5b9a}".to_string(),
        at: rowline_core::input::Point::new(ok.x + 48.0, ok.y + 8.0),
        size: ds.text_size(rowline_theme::TextRole::Body) as u16,
        color: p.text_on_accent.into(),
    });
    cmds
}

/// Draw the menu bar and the open dropdown.
fn menu_commands(game: &Game, layout: Layout) -> Vec<RenderCmd> {
    let mut cmds = Vec::new();
    let ds = &game.design;
    let p = &ds.palette;
    let m = &game.menu.metrics;

    // ---- the bar -------------------------------------------------------
    let bar = game.menu_bar_rect(layout.client_w);
    cmds.push(RenderCmd::RoundedRect {
        rect: bar,
        radius: 0.0,
        color: p.surface_alt.into(),
    });
    // A hairline separates the bar from the content.
    cmds.push(RenderCmd::RoundedRect {
        rect: rowline_core::render::Rect::new(0.0, m.bar_height - 1.0, bar.width, 1.0),
        radius: 0.0,
        color: p.outline.into(),
    });

    for (i, menu) in game.menu.menus.iter().enumerate() {
        let x = i as f32 * m.title_width;
        let rect =
            rowline_core::render::Rect::new(x + 4.0, 4.0, m.title_width - 8.0, m.bar_height - 8.0);
        let open = game.menu.open == Some(i);
        if open {
            cmds.push(RenderCmd::RoundedRect {
                rect,
                radius: ds.radius(rowline_theme::Radius::Sm),
                color: p.surface_hover.into(),
            });
        }
        // Centre the title in the 30 px bar.  The text anchor is the
        // top-left; a 14 px label is about 20 px tall, so 8 px is centred.
        cmds.push(RenderCmd::Text {
            text: menu.title.to_string(),
            at: rowline_core::input::Point::new(
                x + ds.space(rowline_theme::Space::Md),
                m.bar_height * 0.27,
            ),
            size: ds.text_size(rowline_theme::TextRole::Label) as u16,
            color: if open {
                p.text_on_accent.into()
            } else {
                p.text_primary.into()
            },
        });
    }

    // ---- the open dropdown ---------------------------------------------
    let Some(panel) = game.menu_panel_rect() else {
        return cmds;
    };
    cmds.push(RenderCmd::Shadow {
        rect: panel,
        radius: ds.radius(rowline_theme::Radius::Md),
        offset: rowline_core::input::Point::new(0.0, 4.0),
        blur: 16.0,
        color: p.shadow.into(),
    });
    cmds.push(RenderCmd::RoundedRect {
        rect: panel,
        radius: ds.radius(rowline_theme::Radius::Md),
        color: p.surface_raised.into(),
    });
    cmds.push(RenderCmd::Outline {
        rect: panel,
        color: p.outline.into(),
    });

    let Some(open) = game.menu.open else {
        return cmds;
    };
    let Some(menu) = game.menu.menus.get(open) else {
        return cmds;
    };
    for (row, item) in menu.items.iter().enumerate() {
        let y = panel.y + row as f32 * m.row_height;
        let hovered = game.menu.hover == Some(row);
        if hovered {
            cmds.push(RenderCmd::RoundedRect {
                rect: rowline_core::render::Rect::new(
                    panel.x + m.pad * 0.5,
                    y + 2.0,
                    panel.width - m.pad,
                    m.row_height - 4.0,
                ),
                radius: ds.radius(rowline_theme::Radius::Sm),
                color: p.surface_hover.into(),
            });
        }
        cmds.push(RenderCmd::Text {
            text: item.label.to_string(),
            at: rowline_core::input::Point::new(panel.x + m.pad, y + m.row_height * 0.22),
            size: ds.text_size(rowline_theme::TextRole::Body) as u16,
            color: if hovered {
                p.text_on_accent.into()
            } else {
                p.text_primary.into()
            },
        });
        if !item.hint.is_empty() {
            let hint_size = ds.text_size(rowline_theme::TextRole::Caption) as u16;
            let width = 7.0 * item.hint.chars().count() as f32;
            cmds.push(RenderCmd::Text {
                text: item.hint.to_string(),
                at: rowline_core::input::Point::new(
                    panel.x + panel.width - m.pad - width,
                    y + m.row_height * 0.22,
                ),
                size: hint_size,
                color: if hovered {
                    p.text_on_accent.into()
                } else {
                    p.text_tertiary.into()
                },
            });
        }
    }
    cmds
}

/// The minimum client size the layout needs.
pub fn minimum_client_size() -> Size {
    let (w, h) = rowline_core::layout::min_client_size();
    Size::new(w as f32, h as f32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_game_resets_the_selection() {
        let mut g = Game::new(1, RuleModel::default());
        g.selected = Some(4);
        g.run_command(Command::NEW_GAME);
        assert!(g.selected.is_none());
        assert_eq!(g.phase, Phase::Ready);
    }

    #[test]
    fn dialogs_close_from_their_buttons() {
        let layout = Layout::compute_framed(
            760,
            480,
            rowline_core::layout::MENU_BAR_H,
            rowline_core::layout::FRAME_RESERVE,
        );
        let mut g = Game::new(7, RuleModel::default());

        g.run_command(Command::HELP);
        assert_eq!(g.overlay, Overlay::Help);
        let (_panel, button) = help_rects(layout);
        g.overlay_click(
            Point::new(
                button.x + button.width * 0.5,
                button.y + button.height * 0.5,
            ),
            layout,
        );
        assert_eq!(g.overlay, Overlay::None);

        g.run_command(Command::ABOUT);
        let (panel, _) = about_rects(layout);
        let close = close_rect(panel);
        g.overlay_click(
            Point::new(close.x + close.width * 0.5, close.y + close.height * 0.5),
            layout,
        );
        assert_eq!(g.overlay, Overlay::None);

        g.run_command(Command::TOGGLE_STATISTICS);
        let (panel, _) = stats_rects(layout);
        let close = close_rect(panel);
        g.overlay_click(
            Point::new(close.x + close.width * 0.5, close.y + close.height * 0.5),
            layout,
        );
        assert_eq!(g.overlay, Overlay::None);
    }

    #[test]
    fn the_statistics_command_toggles_its_dialog() {
        let mut g = Game::new(1, RuleModel::default());
        assert_eq!(g.overlay, Overlay::None);
        g.run_command(Command::TOGGLE_STATISTICS);
        assert_eq!(g.overlay, Overlay::Statistics);
        g.run_command(Command::TOGGLE_STATISTICS);
        assert_eq!(g.overlay, Overlay::None);
    }

    #[test]
    fn the_hud_shows_three_preview_balls() {
        // The original previews the next group of three (docs/11 section 2.1).
        let g = Game::new(7, RuleModel::default());
        let layout = rowline_core::layout::Layout::compute(1280, 960);
        let cmds = frame(&g, layout);
        let balls = cmds
            .iter()
            .filter(|c| matches!(c, RenderCmd::Sprite { size, .. } if size.width == 24.0))
            .count();
        // Three 24 px preview balls.
        assert!(
            balls >= 3,
            "expected at least the three previews, got {balls}"
        );
        let texts: Vec<&str> = cmds
            .iter()
            .filter_map(|c| match c {
                RenderCmd::Text { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect();
        assert!(
            texts.contains(&"\u{4e0b}\u{4e00}\u{7ec4}"),
            "the next-group label is missing"
        );
    }

    #[test]
    fn the_board_has_a_backdrop_and_one_plate_per_cell() {
        let mut g = Game::new(7, RuleModel::default());
        g.engine_mut().new_game();
        let layout = rowline_core::layout::Layout::compute(1280, 960);
        let cmds = frame(&g, layout);
        let plates = cmds
            .iter()
            .filter(|c| matches!(c, RenderCmd::Sprite { size, .. } if size.width == 36.0))
            .count();
        // 81 board cells + the three HUD preview cells.
        assert!(plates >= 84, "only {plates} plates were drawn");
        // The board strip uses the theme's backdrop token.
        let backdrop: rowline_core::render::Color = g.design.palette.board_backdrop.into();
        assert!(cmds.iter().any(|c| matches!(
            c,
            RenderCmd::RoundedRect { rect, color, .. }
                if rect.x == 0.0
                    && rect.y == layout.board_y as f32
                    && rect.width == layout.client_w as f32
                    && *color == backdrop
        )));
    }

    fn cell_centre(layout: Layout, r: i32, c: i32) -> Point {
        let (x, y) = layout.cell_origin(r, c);
        Point::new(x as f32 + 18.0, y as f32 + 18.0)
    }

    #[test]
    fn clicking_another_ball_switches_the_selection() {
        let mut g = Game::new(7, RuleModel::default());
        {
            let b = g.engine_mut().board_mut();
            b.cells_mut().fill(0);
            b.set(0, 0, 1);
            b.set(0, 5, 2);
        }
        let layout = Layout::compute_framed(
            604,
            396,
            rowline_core::layout::MENU_BAR_H,
            rowline_core::layout::FRAME_RESERVE,
        );
        g.click_at(cell_centre(layout, 0, 0), layout);
        assert_eq!(g.selected, Some(rowline_core::index(0, 0)));
        // Selecting a second ball switches to it instead of trying to move.
        g.click_at(cell_centre(layout, 0, 5), layout);
        assert_eq!(g.selected, Some(rowline_core::index(0, 5)));
        // The board is untouched.
        assert_eq!(g.engine().board().colour_at(0, 0), 1);
        assert_eq!(g.engine().board().colour_at(0, 5), 2);
        // Clicking the selected ball again clears the selection.
        g.click_at(cell_centre(layout, 0, 5), layout);
        assert_eq!(g.selected, None);
    }

    #[test]
    fn selecting_a_ball_starts_a_pick_up_pulse() {
        let mut g = Game::new(7, RuleModel::default());
        {
            let b = g.engine_mut().board_mut();
            b.cells_mut().fill(0);
            b.set(1, 1, 3);
        }
        let layout = Layout::compute_framed(
            604,
            396,
            rowline_core::layout::MENU_BAR_H,
            rowline_core::layout::FRAME_RESERVE,
        );
        g.click_at(cell_centre(layout, 1, 1), layout);
        assert!(g.select_anim.is_some());
        assert!(g.is_animating());
        let cmds = frame(&g, layout);
        assert!(
            cmds.iter()
                .filter(|c| matches!(c, RenderCmd::Outline { .. }))
                .count()
                >= 2,
            "the selection ring and the pulse ring should both be drawn"
        );
    }

    #[test]
    fn pinyin_typed_during_a_composition_is_replaced_by_the_commit() {
        use rowline_core::input::{InputEvent, Key, Modifiers};

        let layout = Layout::compute_framed(
            604,
            396,
            rowline_core::layout::MENU_BAR_H,
            rowline_core::layout::FRAME_RESERVE,
        );
        let mut g = Game::new(7, RuleModel::default());
        g.offer_name_entry();

        // The IME reports the composition, then the physical keys deliver
        // the pinyin letters.
        g.handle_event(
            &InputEvent::ImePreedit {
                text: "jian".to_string(),
            },
            layout,
        );
        for key in [Key::KeyJ, Key::KeyI, Key::KeyA, Key::KeyN] {
            g.handle_event(
                &InputEvent::KeyPressed {
                    key,
                    modifiers: Modifiers::default(),
                },
                layout,
            );
        }
        // The commit replaces the raw pinyin with the characters.
        g.handle_event(
            &InputEvent::TextInput {
                text: "剑客".to_string(),
            },
            layout,
        );
        g.handle_event(
            &InputEvent::ImePreedit {
                text: String::new(),
            },
            layout,
        );
        // The selected default was replaced, not appended to.
        assert_eq!(g.name_input, "剑客");
    }

    #[test]
    fn a_commit_without_a_preedit_also_drops_the_pinyin() {
        use rowline_core::input::{InputEvent, Key, Modifiers};

        let layout = Layout::compute_framed(
            604,
            396,
            rowline_core::layout::MENU_BAR_H,
            rowline_core::layout::FRAME_RESERVE,
        );
        let mut g = Game::new(7, RuleModel::default());
        g.offer_name_entry();
        for key in [Key::KeyL, Key::KeyI, Key::KeyN] {
            g.handle_event(
                &InputEvent::KeyPressed {
                    key,
                    modifiers: Modifiers::default(),
                },
                layout,
            );
        }
        assert!(g.name_input.ends_with("lin"));
        g.handle_event(
            &InputEvent::TextInput {
                text: "林".to_string(),
            },
            layout,
        );
        assert!(g.name_input.ends_with("林"), "got {:?}", g.name_input);
        assert!(!g.name_input.contains("lin"));
    }

    #[test]
    fn the_leaderboard_buttons_stay_inside_the_panel() {
        let layout = Layout::compute_framed(
            604,
            396,
            rowline_core::layout::MENU_BAR_H,
            rowline_core::layout::FRAME_RESERVE,
        );
        let (panel, _) = scoreboard_rects(layout);
        let (clear, ok) = scoreboard_buttons((panel, 0.0));
        for b in [clear, ok] {
            assert!(b.x >= panel.x, "{b:?} starts left of {panel:?}");
            assert!(b.x + b.width <= panel.x + panel.width, "{b:?} overflows");
            assert!(b.y + b.height <= panel.y + panel.height, "{b:?} overflows");
        }
    }

    #[test]
    fn acknowledging_the_leaderboard_starts_a_new_round() {
        use rowline_core::input::{InputEvent, Modifiers, MouseButton};

        let layout = Layout::compute_framed(
            604,
            396,
            rowline_core::layout::MENU_BAR_H,
            rowline_core::layout::FRAME_RESERVE,
        );
        let mut g = Game::new(7, RuleModel::default());
        g.set_game_over(true);
        // The dialog has already been acknowledged; the table is open.
        g.game_over_ack = true;
        let (panel, row_h) = scoreboard_rects(layout);
        let (_clear, ok) = scoreboard_buttons((panel, row_h));
        g.overlay = Overlay::Scoreboard;
        let before = g.engine().step();
        g.handle_event(
            &InputEvent::MousePressed {
                pos: Point::new(ok.x + 4.0, ok.y + 4.0),
                button: MouseButton::Left,
                modifiers: Modifiers::default(),
            },
            layout,
        );
        let _ = before;
        assert_eq!(g.overlay, Overlay::None);
        assert!(!g.game_over);
        assert_eq!(g.phase, Phase::Ready);
        // A fresh round puts exactly five balls back on the board.
        let balls = g
            .engine()
            .board()
            .cells()
            .iter()
            .filter(|&&c| c != 0)
            .count();
        assert_eq!(balls, 5, "a new round should have started");
    }

    #[test]
    fn a_move_animates_before_the_engine_changes_the_board() {
        let mut g = Game::new(7, RuleModel::default());
        {
            let b = g.engine_mut().board_mut();
            b.cells_mut().fill(0);
            b.set(0, 0, 1);
        }
        let layout = Layout::compute_framed(
            604,
            396,
            rowline_core::layout::MENU_BAR_H,
            rowline_core::layout::FRAME_RESERVE,
        );
        g.click_at(cell_centre(layout, 0, 0), layout);
        g.click_at(cell_centre(layout, 0, 2), layout);
        // The visual move is in flight; the source ball is still in the
        // engine's board so the renderer can cover it.
        assert!(g.move_anim.is_some());
        assert_eq!(g.engine().board().colour_at(0, 0), 1);
        assert_eq!(g.engine().board().colour_at(0, 2), 0);

        // Rewind the clock and let the tick finish the move.
        if let Some(anim) = g.move_anim.as_mut() {
            anim.started = Instant::now() - anim.duration() - Duration::from_millis(1);
        }
        g.tick(0.016, layout);
        assert!(g.move_anim.is_none());
        assert_eq!(g.engine().board().colour_at(0, 0), 0);
        assert_eq!(g.engine().board().colour_at(0, 2), 1);
    }

    #[test]
    fn a_qualifying_game_over_accepts_a_typed_and_clicked_name() {
        use rowline_core::input::{InputEvent, Key, Modifiers};

        let layout = Layout::compute_framed(
            604,
            396,
            rowline_core::layout::MENU_BAR_H,
            rowline_core::layout::FRAME_RESERVE,
        );
        let mut g = Game::new(7, RuleModel::default());
        g.engine_mut().set_score(42);
        g.set_game_over(true);

        // Dismissing the game-over dialog opens the name entry because the
        // score beats the empty rows.
        g.handle_event(
            &InputEvent::KeyPressed {
                key: Key::Enter,
                modifiers: Modifiers::default(),
            },
            layout,
        );
        assert_eq!(g.overlay, Overlay::NameEntry);

        // Latin typing goes through the physical keys.
        for key in [Key::KeyX, Key::KeyY] {
            g.handle_event(
                &InputEvent::KeyPressed {
                    key,
                    modifiers: Modifiers::default(),
                },
                layout,
            );
        }
        assert!(g.name_input.ends_with("xy"));

        // The 确定(O) button is a drawn control and must accept a click.
        let (_panel, _field, ok) = name_entry_rects(layout);
        g.handle_event(
            &InputEvent::MousePressed {
                pos: Point::new(ok.x + 4.0, ok.y + 4.0),
                button: MouseButton::Left,
                modifiers: Modifiers::default(),
            },
            layout,
        );
        assert_eq!(g.overlay, Overlay::Scoreboard);
        assert!(g
            .scores
            .iter()
            .any(|r| r.name.ends_with("xy") && r.score == 42));
    }

    #[test]
    fn the_name_field_is_white_with_black_text() {
        let layout = Layout::compute_framed(
            604,
            396,
            rowline_core::layout::MENU_BAR_H,
            rowline_core::layout::FRAME_RESERVE,
        );
        let mut g = Game::new(7, RuleModel::default());
        let field_colour = rowline_core::render::Color::WHITE;
        let text_colour = rowline_core::render::Color::BLACK;
        g.offer_name_entry();
        // Clear the selected default so the caret/black-text state is drawn.
        g.name_key(rowline_core::input::Key::Backspace);
        let cmds = frame(&g, layout);
        let (_panel, field, _ok) = name_entry_rects(layout);
        assert!(
            cmds.iter().any(|c| matches!(
                c,
                RenderCmd::RoundedRect { rect, color, .. }
                    if *rect == field && *color == field_colour
            )),
            "the edit field must be white"
        );
        assert!(
            cmds.iter().any(|c| matches!(
                c,
                RenderCmd::Text { text, color, .. }
                    if text == &g.name_input && *color == text_colour
            )),
            "the name must be black on the white field"
        );
    }

    #[test]
    fn the_name_entry_shows_a_selection_then_a_caret() {
        let layout = Layout::compute_framed(
            604,
            396,
            rowline_core::layout::MENU_BAR_H,
            rowline_core::layout::FRAME_RESERVE,
        );
        let mut g = Game::new(7, RuleModel::default());
        g.offer_name_entry();
        let (_panel, field, _ok) = name_entry_rects(layout);
        let accent: rowline_core::render::Color = g.design.palette.accent.into();
        let selected = frame(&g, layout);
        assert!(
            selected.iter().any(|c| matches!(
                c,
                RenderCmd::RoundedRect { rect, color, .. }
                    if rect.x == field.x + 4.0 && *color == accent
            )),
            "the default name must be shown as selected"
        );

        g.name_key(rowline_core::input::Key::Backspace);
        let cleared = frame(&g, layout);
        assert!(
            cleared.iter().any(|c| matches!(
                c,
                RenderCmd::RoundedRect { rect, .. } if rect.width == 1.5
            )),
            "the caret must be drawn after the default is cleared"
        );
    }

    #[test]
    fn ime_text_is_appended_to_the_name_entry() {
        use rowline_core::input::InputEvent;

        let layout = Layout::compute_framed(
            604,
            396,
            rowline_core::layout::MENU_BAR_H,
            rowline_core::layout::FRAME_RESERVE,
        );
        let mut g = Game::new(7, RuleModel::default());
        g.offer_name_entry();
        g.handle_event(
            &InputEvent::TextInput {
                text: "张三".to_string(),
            },
            layout,
        );
        assert!(g.name_input.ends_with("张三"));
        // Pure ASCII is handled by the physical keys; accepting it here too
        // would duplicate every Latin character.
        g.handle_event(
            &InputEvent::TextInput {
                text: "abc".to_string(),
            },
            layout,
        );
        assert!(!g.name_input.ends_with("abc"));
    }

    #[test]
    fn the_original_accelerators_work() {
        use rowline_core::input::{InputEvent, Key, Modifiers};
        let layout = Layout::compute(604, 371);
        let alt = Modifiers {
            alt: true,
            ..Default::default()
        };

        // Alt+F9 quits, as in the original's 文件 · 退出.
        let mut g = Game::new(7, RuleModel::default());
        g.handle_event(
            &InputEvent::KeyPressed {
                key: Key::F9,
                modifiers: alt,
            },
            layout,
        );
        assert!(g.quit_requested());

        // H is the original's mnemonic for 显示高分榜.
        let mut g = Game::new(7, RuleModel::default());
        g.handle_event(
            &InputEvent::KeyPressed {
                key: Key::KeyH,
                modifiers: Modifiers::default(),
            },
            layout,
        );
        assert_eq!(g.overlay, Overlay::Scoreboard);

        // Alt+O opens 选项.
        let mut g = Game::new(7, RuleModel::default());
        g.handle_event(
            &InputEvent::KeyPressed {
                key: Key::KeyO,
                modifiers: alt,
            },
            layout,
        );
        assert_eq!(g.menu.open, Some(1));
    }

    #[test]
    fn the_menu_opens_on_click_not_on_hover() {
        let mut g = Game::new(7, RuleModel::default());
        // Hover alone must leave a closed menu closed, otherwise the click
        // that follows toggles it straight shut again.
        g.menu_hover(Point::new(30.0, 10.0));
        assert!(g.menu.is_closed());
        g.menu_click(Point::new(30.0, 10.0));
        assert_eq!(g.menu.open, Some(0));
        // Once open, moving across the bar switches menus.
        g.menu_hover(Point::new(100.0, 10.0));
        assert_eq!(g.menu.open, Some(1));
    }

    #[test]
    fn hovering_previews_the_path_from_the_selected_ball() {
        let mut g = Game::new(7, RuleModel::default());
        {
            let b = g.engine_mut().board_mut();
            b.cells_mut().fill(0);
            b.set(0, 0, 1);
        }
        let layout = rowline_core::layout::Layout::compute(1280, 960);
        g.selected = Some(rowline_core::index(0, 0));
        let (dx, dy) = layout.cell_origin(2, 2);
        g.update_hover(Point::new(dx as f32 + 18.0, dy as f32 + 18.0), layout);
        assert_eq!(g.hovered(), Some(rowline_core::index(2, 2)));
        assert_eq!(g.path.first(), Some(&rowline_core::index(0, 0)));
        assert_eq!(g.path.last(), Some(&rowline_core::index(2, 2)));
        // The hover ring and the path are both in the frame.
        let cmds = frame(&g, layout);
        let accent: rowline_core::render::Color = g.design.palette.accent.into();
        assert!(
            cmds.iter()
                .any(|c| matches!(c, RenderCmd::Outline { color, .. } if *color == accent)),
            "the hover ring is missing"
        );
    }

    #[test]
    fn a_clear_dissolves_in_two_steps() {
        let mut g = Game::new(7, RuleModel::default());
        g.clear_anim = Some(ClearAnimation {
            started: Instant::now(),
            cells: vec![(rowline_core::index(0, 0), 1)],
        });
        let layout = rowline_core::layout::Layout::compute(1280, 960);
        let colour: rowline_core::render::Color = g.design.palette.balls.get(1).into();
        let fragments = |cmds: &[RenderCmd]| {
            cmds.iter()
                .filter(|c| matches!(c, RenderCmd::RoundedRect { color, .. } if *color == colour))
                .count()
        };
        // Phase 0: five of the nine fragments.
        assert_eq!(fragments(&frame(&g, layout)), 5);
        // Skip to phase 1: three fragments survive.
        if let Some(a) = g.clear_anim.as_mut() {
            a.started = Instant::now() - CLEAR_STEP;
        }
        assert_eq!(fragments(&frame(&g, layout)), 3);
    }

    #[test]
    fn the_statistics_panel_is_opt_in_and_lists_every_slot() {
        let mut g = Game::new(7, RuleModel::default());
        let layout = rowline_core::layout::Layout::compute(1280, 960);
        // Statistics is a modal opened from the menu / F5, not a persisted
        // setting.
        assert_eq!(g.overlay, Overlay::None);
        let texts = |cmds: &[RenderCmd]| -> Vec<String> {
            cmds.iter()
                .filter_map(|c| match c {
                    RenderCmd::Text { text, .. } => Some(text.clone()),
                    _ => None,
                })
                .collect()
        };
        assert!(!texts(&frame(&g, layout)).iter().any(|t| t == "统计"));

        g.run_command(Command::TOGGLE_STATISTICS);
        {
            let b = g.engine_mut().board_mut();
            b.cells_mut().fill(0);
            b.set(0, 0, 1);
        }
        let cmds = frame(&g, layout);
        let texts = texts(&cmds);
        assert!(texts.iter().any(|t| t == "统计"));
        // 80 empty cells of 81 -> 98%, one green ball -> 1%.  The redesign
        // draws the count and the share in separate columns.
        assert!(
            texts.iter().any(|t| t == "\u{7a7a}\u{683c}"),
            "the empty-cell row is missing: {texts:?}"
        );
        assert!(texts.iter().any(|t| t == "80"), "empty count missing");
        assert!(texts.iter().any(|t| t == "98%"), "empty share missing");
        assert!(texts.iter().any(|t| t == "1"), "colour count missing");
        // Seven ball rows plus the empty row.
        assert_eq!(
            texts.iter().filter(|t| t.contains('%')).count(),
            8,
            "the panel must have eight statistic rows"
        );
    }

    #[test]
    fn the_game_over_dialog_appears_when_there_is_no_move() {
        let mut g = Game::new(7, RuleModel::default());
        g.set_game_over(true);
        let layout = rowline_core::layout::Layout::compute(1280, 960);
        let cmds = frame(&g, layout);
        let texts: Vec<&str> = cmds
            .iter()
            .filter_map(|c| match c {
                RenderCmd::Text { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect();
        assert!(
            texts.contains(&"\u{6e38}\u{620f}\u{7ed3}\u{675f}"),
            "the dialog title is missing"
        );
        assert!(
            texts.contains(&"\u{65e0}\u{6cd5}\u{79fb}\u{52a8}\u{4e86}"),
            "the dialog text is missing"
        );
        assert!(
            texts.contains(&"\u{786e}\u{5b9a}"),
            "the OK button is missing"
        );
    }

    #[test]
    fn the_leaderboard_shows_sixteen_rows_and_the_empty_placeholder() {
        let mut g = Game::new(7, RuleModel::default());
        g.scores.push(ScoreRecord {
            name: "挑战者".into(),
            score: 160,
        });
        g.scores.push(ScoreRecord {
            name: "国王".into(),
            score: 100,
        });
        g.run_command(crate::commands::Command::SHOW_HIGH_SCORES);
        let layout = rowline_core::layout::Layout::compute(1280, 960);
        let cmds = frame(&g, layout);
        let texts: Vec<&str> = cmds
            .iter()
            .filter_map(|c| match c {
                RenderCmd::Text { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect();
        assert!(texts.contains(&"\u{603b}\u{699c}"), "the title is missing");
        assert!(
            texts.iter().any(|t| t.starts_with("1. 挑战者")),
            "the first row is missing: {texts:?}"
        );
        assert!(
            texts.iter().any(|t| t.contains("- 空的 -")),
            "the empty placeholder is missing"
        );
    }

    #[test]
    fn the_name_entry_starts_with_the_originals_default_name() {
        let mut g = Game::new(7, RuleModel::default());
        g.engine_mut().set_score(120);
        g.offer_name_entry();
        assert_eq!(g.overlay, Overlay::NameEntry);
        assert_eq!(g.name_input, "挑战者");
        assert!(g.name_select_all);
        // The default is selected: the first character replaces it.
        g.name_key(rowline_core::input::Key::KeyA);
        assert_eq!(g.name_input, "a");
        assert!(!g.name_select_all);
        g.name_key(rowline_core::input::Key::Backspace);
        assert_eq!(g.name_input, "");
        g.accept_name();
        // The original shows the leaderboard right after the name is entered.
        assert_eq!(g.overlay, Overlay::Scoreboard);
        // The new record joins the CPU's default entry.
        assert_eq!(g.scores().len(), 2);
        assert_eq!(g.scores()[0].name, "挑战者");
        assert_eq!(g.scores()[0].score, 120);
        assert_eq!(g.scores()[1].name, "国王");
        assert_eq!(g.scores()[1].score, 100);
    }

    #[test]
    fn the_score_is_only_inserted_when_the_game_is_over() {
        let mut g = Game::new(7, RuleModel::default());
        g.engine_mut().set_score(120);
        // A qualifying score does not open the name entry mid-game.
        assert_eq!(g.overlay, Overlay::None);
        assert_eq!(g.scores().len(), 1);

        g.set_game_over(true);
        // The message is modal until acknowledged.
        assert!(g.game_over && !g.game_over_ack);
        g.ack_game_over();
        assert_eq!(g.overlay, Overlay::NameEntry);
        g.accept_name();
        assert_eq!(g.overlay, Overlay::Scoreboard);
        assert_eq!(g.scores().len(), 2);
    }

    #[test]
    fn a_losing_score_does_not_open_the_name_entry() {
        let mut g = Game::new(7, RuleModel::default());
        g.engine_mut().set_score(0);
        g.set_game_over(true);
        g.ack_game_over();
        assert_eq!(g.overlay, Overlay::None);
    }

    #[test]
    fn the_hud_shows_the_record_on_the_left_and_the_round_on_the_right() {
        // The left box is the record from the leaderboard; the right one is
        // the current round.  The record does **not** move while the round
        // is played, even when the round beats it.
        let mut g = Game::new(7, RuleModel::default());
        g.engine_mut().set_score(50);
        assert_eq!(g.king_score(), 100);
        assert_eq!(g.challenger_score(), 50);
        g.engine_mut().set_score(160);
        assert_eq!(g.king_score(), 100);
        assert_eq!(g.challenger_score(), 160);
    }

    #[test]
    fn the_king_starts_higher_than_the_challenger() {
        let g = Game::new(7, RuleModel::default());
        let (king, challenger) = g.figure_heights();
        // The default record is 100 points; the round has not started.
        assert!((king - 100.0).abs() < 0.01, "king {king}");
        assert_eq!(challenger, 0.0);
    }

    #[test]
    fn the_challenger_rises_with_the_score_and_stops_at_150_percent() {
        let mut g = Game::new(7, RuleModel::default());
        let layout = Layout::compute_framed(604, 396, rowline_core::layout::MENU_BAR_H, 4);

        g.engine_mut().set_score(50);
        g.tick(1.0, layout);
        assert!((g.figure_heights().1 - 50.0).abs() < 0.5);

        g.engine_mut().set_score(120);
        g.tick(1.0, layout);
        let (king, challenger) = g.figure_heights();
        assert!(challenger > king, "the challenger should overtake the king");

        // 150 % of the 100-point record is the ceiling.
        g.engine_mut().set_score(400);
        g.tick(1.0, layout);
        assert!((g.figure_heights().1 - 150.0).abs() < 0.5);
    }

    #[test]
    fn both_figures_use_the_same_scale() {
        // Equal scores stand equally high, whatever the record is.
        assert!((score_to_height(100, 100) - 100.0).abs() < 0.01);
        assert!((score_to_height(200, 200) - 100.0).abs() < 0.01);
        assert!((score_to_height(100, 200) - 50.0).abs() < 0.01);
        assert!((score_to_height(300, 200) - 150.0).abs() < 0.01);
        // 150 % of any record is the full column.
        assert!((score_to_height(999, 100) - 150.0).abs() < 0.01);
    }

    #[test]
    fn a_new_record_drops_both_figures_then_raises_the_king() {
        let mut g = Game::new(7, RuleModel::default());
        let layout = Layout::compute_framed(604, 396, rowline_core::layout::MENU_BAR_H, 4);
        g.engine_mut().set_score(160);
        g.finish_figure_animation();
        let (king_before, challenger_before) = g.figure_heights();
        assert!(challenger_before > king_before);
        // Under the old scale the challenger is capped at the full column.
        assert!((challenger_before - 150.0).abs() < 0.5);

        // The record is set and a new game starts: both figures drop, then
        // the king climbs to the new scale's record height (2/3 column).
        g.start_new_game();
        assert!(g.reset_anim.is_some(), "the reset animation must start");
        g.tick(0.0, layout);
        // Halfway through the drop phase.
        if let Some(anim) = g.reset_anim.as_mut() {
            anim.started = Instant::now() - RESET_DROP / 2;
        }
        g.tick(0.0, layout);
        let (king_mid, challenger_mid) = g.figure_heights();
        assert!(king_mid < king_before * 0.75, "the king is dropping");
        assert!(
            challenger_mid < challenger_before * 0.75,
            "the challenger is dropping"
        );
        assert!(
            g.displayed_king_score() < king_before as u32,
            "the record counter drops with the king"
        );

        // After both phases the king is back at the record height and the
        // challenger is on the floor.
        if let Some(anim) = g.reset_anim.as_mut() {
            anim.started = Instant::now() - RESET_DROP - RESET_RISE - Duration::from_millis(1);
        }
        g.tick(0.0, layout);
        let (king, challenger) = g.figure_heights();
        assert!((king - 100.0).abs() < 0.5, "king {king}");
        assert!(challenger < 0.5, "challenger {challenger}");
        assert!(g.reset_anim.is_none());
        assert_eq!(g.displayed_king_score(), 160);
        assert_eq!(g.king_score(), 160);
    }

    #[test]
    fn beating_the_record_starts_the_celebration() {
        let mut g = Game::new(7, RuleModel::default());
        g.engine_mut().set_score(99);
        {
            let b = g.engine_mut().board_mut();
            b.cells_mut().fill(0);
            for c in 0..4 {
                b.set(4, c, 1);
            }
            b.set(5, 4, 1);
        }
        let layout = Layout::compute_framed(
            604,
            396,
            rowline_core::layout::MENU_BAR_H,
            rowline_core::layout::FRAME_RESERVE,
        );
        g.click_at(cell_centre(layout, 5, 4), layout);
        g.click_at(cell_centre(layout, 4, 4), layout);
        // Let the move animation finish so the engine applies the clear.
        if let Some(anim) = g.move_anim.as_mut() {
            anim.started = Instant::now() - Duration::from_secs(5);
        }
        g.tick(0.0, layout);
        // The clear scores 10, so 109 beats the default record of 100.  The
        // left box keeps showing the old record until a new game rescales.
        assert!(g.record_celebrating());
        assert_eq!(g.king_score(), 100);
        assert_eq!(g.challenger_score(), 109);
    }

    #[test]
    fn exit_command_requests_quit() {
        let mut g = Game::new(1, RuleModel::default());
        assert!(!g.quit_requested());
        g.run_command(Command::EXIT);
        assert!(g.quit_requested());
    }
}
