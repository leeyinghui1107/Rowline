//! The game: state machine, input handling and the frame loop.
//!
//! The game object owns the engine and the settings and knows how to turn
//! events into engine operations.  It does **not** know how to draw (that is
//! `rowline_core::view`) nor how to read files (that is `rowline-platform`).

use rowline_core::engine::Engine;
use rowline_core::input::{InputEvent, MouseButton, Point, Size};
use rowline_core::layout::Layout;
use rowline_core::render::{FrameStyle, RenderCmd};
use rowline_core::state::{GameState, Phase};
use std::time::{Duration, Instant};

use rowline_core::{RuleModel, ScoreRecord, Settings};

use rowline_theme::{DesignSystem, ThemeKind};

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
    /// `true` while the command reference is shown.
    pub help_open: bool,
    /// `true` when no move is left and the game is over.
    pub game_over: bool,
    /// `true` once the end-of-game message has been acknowledged.
    pub game_over_ack: bool,
    /// The overlay the user is interacting with.
    pub overlay: Overlay,
    /// The name being typed in the name-entry dialog.
    pub name_input: String,
    /// `true` when the viewport must be recomputed (the display policy
    /// changed).  Cleared by [`Game::take_viewport_dirty`].
    viewport_dirty: bool,
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
            // The original's Win95 look is the default; the redesigned dark
            // and light themes remain available through Ctrl+T.
            design: DesignSystem::new(ThemeKind::Classic, false),
            menu: rowline_core::menu::MenuModel::new(rowline_core::menu::default_menus()),
            effects: rowline_core::effects::EffectSystem::new(0x5EED),
            help_open: false,
            game_over: false,
            game_over_ack: true,
            overlay: Overlay::None,
            name_input: rowline_core::scoreboard::default_player_name().to_string(),
            viewport_dirty: false,
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

    /// The integer scale of the loaded sprite sheet.
    pub fn sheet_scale(&self) -> u32 {
        self.sheet_scale
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
        if self.design.motion.reduced || t >= drop + rise {
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
            let anim = self.move_anim.take().expect("checked above");
            if let (Some(&src), Some(&dst)) = (anim.path.first(), anim.path.last()) {
                let (sr, sc) = rowline_core::coords(src);
                let (dr, dc) = rowline_core::coords(dst);
                let res = self
                    .engine
                    .move_ball(sr as i32, sc as i32, dr as i32, dc as i32);
                self.apply_move_result(res, anim.colour, layout);
            }
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
            if self.design.motion.reduced || t >= drop + rise {
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
            if self.design.motion.reduced {
                self.king_rise = self.king_rise_target;
                self.challenger_rise = self.challenger_rise_target;
            } else {
                let k = 1.0 - (-dt / FIGURE_TAU).exp();
                self.king_rise += (self.king_rise_target - self.king_rise) * k;
                self.challenger_rise += (self.challenger_rise_target - self.challenger_rise) * k;
            }
        }
        // With reduced motion the system simply never holds any particle, so
        // there is nothing to update.
        if self.design.motion.reduced {
            self.effects.clear();
            return;
        }
        self.effects.update(dt);
    }

    /// Spawn the burst for a cleared cell.
    fn burst_at(&mut self, row: usize, col: usize, layout: Layout) {
        if self.design.motion.reduced {
            return;
        }
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

    /// The rectangle of the toolbar at the right of the menu bar.
    pub fn menu_toolbar_rect(&self, client_w: i32) -> rowline_core::render::Rect {
        let m = &self.menu.metrics;
        let w = 2.0 * (m.bar_height - 6.0);
        rowline_core::render::Rect::new(client_w as f32 - w - m.pad, 3.0, w, m.bar_height - 6.0)
    }

    /// Which toolbar button is under the pointer, if any.
    ///
    /// `0` = theme, `1` = display (retro mode).
    pub fn toolbar_button_at(&self, pos: Point, client_w: i32) -> Option<usize> {
        let r = self.menu_toolbar_rect(client_w);
        if !r.contains(pos) {
            return None;
        }
        let w = (r.width * 0.5).max(1.0);
        if pos.x < r.x + w {
            Some(0)
        } else {
            Some(1)
        }
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

    /// Switch between the dark and the light theme.
    pub fn toggle_theme(&mut self) {
        let next = self.design.kind().toggled();
        self.set_theme(next);
    }

    /// Select a theme explicitly (used by the head-less screenshot tool).
    pub fn set_theme(&mut self, kind: ThemeKind) {
        self.design.set_theme(kind);
        self.viewport_dirty = true;
        self.sync_settings();
    }

    /// Toggle the reduced-motion preference.
    pub fn toggle_reduced_motion(&mut self) {
        let reduced = !self.design.motion.reduced;
        self.design.set_reduced_motion(reduced);
        self.sync_settings();
    }

    /// Cycle the text-size preference.
    pub fn cycle_text_scale(&mut self) {
        let all = rowline_theme::TextScale::ALL;
        let cur = self.design.tokens.text_scale;
        let idx = all.iter().position(|t| *t == cur).unwrap_or(1);
        self.design.set_text_scale(all[(idx + 1) % all.len()]);
        self.sync_settings();
    }

    /// Toggle the fixed-resolution ("retro") scaling policy.
    pub fn toggle_retro_mode(&mut self) {
        self.settings.retro_mode = !self.settings.retro_mode;
        self.viewport_dirty = true;
    }

    /// `true` once after the display policy changed, then clears the flag.
    pub fn take_viewport_dirty(&mut self) -> bool {
        std::mem::take(&mut self.viewport_dirty)
    }

    /// Apply the persisted settings to the design system.
    pub fn apply_settings(&mut self) {
        let theme = match self.settings.theme {
            0 => ThemeKind::Dark,
            1 => ThemeKind::Light,
            _ => ThemeKind::Classic,
        };
        self.design.set_theme(theme);
        self.design.set_reduced_motion(self.settings.reduced_motion);
        let scales = rowline_theme::TextScale::ALL;
        let idx = (self.settings.text_scale as usize).min(scales.len() - 1);
        self.design.set_text_scale(scales[idx]);
        self.engine.set_use_figures(self.settings.use_figures);
        self.viewport_dirty = true;
    }

    /// Copy the live design state back into the settings.
    pub fn sync_settings(&mut self) {
        self.settings.theme = match self.design.kind() {
            ThemeKind::Dark => 0,
            ThemeKind::Light => 1,
            ThemeKind::Classic => 2,
        };
        self.settings.reduced_motion = self.design.motion.reduced;
        self.settings.text_scale = match self.design.tokens.text_scale {
            rowline_theme::TextScale::Small => 0,
            rowline_theme::TextScale::Normal => 1,
            rowline_theme::TextScale::Large => 2,
            rowline_theme::TextScale::ExtraLarge => 3,
        };
        self.settings.use_figures = self.engine.use_figures();
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
        self.apply_settings();
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
            show_next: self.settings.show_next,
            show_statistics: self.settings.show_statistics,
            use_figures: self.engine.use_figures(),
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
                    if self.menu_click(*pos, layout.client_w) {
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
                    } else if self.ime_start.is_none() {
                        self.ime_start = Some(self.name_input.len());
                    }
                }
            }
            InputEvent::TextInput { text } => {
                if self.overlay != Overlay::NameEntry {
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

    /// Execute a menu command.
    pub fn run_command(&mut self, cmd: Command) {
        match cmd {
            Command::NEW_GAME => self.start_new_game(),
            Command::TOGGLE_NEXT_SET => {
                self.settings.show_next = !self.settings.show_next;
            }
            Command::TOGGLE_STATISTICS => {
                self.settings.show_statistics = !self.settings.show_statistics;
            }
            Command::TOGGLE_USE_FIGURES => {
                let next = !self.engine.use_figures();
                self.engine.set_use_figures(next);
                self.settings.use_figures = next;
            }
            Command::TOGGLE_FIXED_SCALE => self.toggle_retro_mode(),
            Command::EXIT => self.quit = true,
            Command::SHOW_HIGH_SCORES => {
                self.overlay = Overlay::Scoreboard;
            }
            Command::ABOUT => {
                self.overlay = Overlay::About;
            }
            Command::CONTENTS => {
                // The original opens `rowline.hlp`; the re-implementation
                // shows the command reference in game.
                self.help_open = !self.help_open;
            }
            Command::HELP => {
                // The original opens `rowline.hlp`; the re-implementation
                // shows the command reference in game.
                self.help_open = !self.help_open;
            }
            // `Command` is an open tuple struct, so an unknown id must be
            // tolerated rather than treated as a programming error.
            _ => {}
        }
    }

    /// `true` when the click was consumed by the menu.
    fn menu_click(&mut self, pos: Point, client_w: i32) -> bool {
        let m = self.menu.metrics;
        // The toolbar, at the right of the menu bar.  It is hidden in the
        // classic theme (the original has no toolbar), so it must not
        // swallow clicks there either.
        if !self.design.is_classic() {
            if let Some(btn) = self.toolbar_button_at(pos, client_w) {
                match btn {
                    0 => self.toggle_theme(),
                    _ => self.toggle_retro_mode(),
                }
                return true;
            }
        }
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
                let cmd = self.menu.click(pos.x - panel.x, pos.y - panel.y);
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
                self.menu.hover_at(pos.x - panel.x, pos.y - panel.y);
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
                } else if self.help_open {
                    self.help_open = false;
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
                self.name_input.pop();
            }
            Key::Escape => self.dismiss_overlay(),
            _ => {
                // Only printable characters are accepted; the name is capped.
                if let Some(ch) = key_char(key) {
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
                self.select_anim = if self.design.motion.reduced {
                    None
                } else {
                    Some((cell, Instant::now()))
                };
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
                if self.design.motion.reduced {
                    // No animation: apply the engine move immediately.
                    let res = self.engine.move_ball(pr as i32, pc as i32, r, c);
                    self.apply_move_result(res, moving_colour, layout);
                } else {
                    self.move_anim = Some(MoveAnimation {
                        path,
                        colour: moving_colour,
                        started: Instant::now(),
                    });
                }
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
    }

    /// Accept the typed name and record it, then show the leaderboard.
    ///
    /// The original writes the row and then shows the table (`0x402D4A`),
    /// which is why accepting a name lands on the leaderboard.
    pub fn accept_name(&mut self) {
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
            if self.design.motion.reduced {
                self.king_rise = self.king_rise_target;
                self.challenger_rise = 0.0;
            } else {
                self.reset_anim = Some(ScaleReset {
                    started: Instant::now(),
                    king_from,
                    challenger_from,
                    record_from,
                });
            }
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
                let (_panel, _field, ok) = name_entry_rects(layout);
                if ok.contains(pos) {
                    self.accept_name();
                }
                // Modal: never let a click reach the board behind it.
                true
            }
            Overlay::Scoreboard => {
                let (clear, ok) =
                    scoreboard_buttons(scoreboard_rects(layout, self.design.is_classic()));
                if clear.contains(pos) {
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
                self.dismiss_overlay();
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
/// The board itself is drawn by the shared view builder; the **HUD** (the
/// side panel) is drawn here with the modern design language: rounded
/// surfaces, a soft shadow and the score in the numeric style.
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

    // ---- the statistics window (非模态, doc/11 section 5e) --------------
    if game.settings.show_statistics {
        cmds.extend(stats_panel(game, layout));
    }

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

    // ---- the command reference -----------------------------------------
    if game.help_open {
        cmds.extend(help_overlay(game, layout));
    }

    // ---- the end-of-game dialog ----------------------------------------
    if game.game_over && !game.game_over_ack {
        cmds.extend(game_over_dialog(game, layout));
    }

    // ---- the modal overlays -------------------------------------------
    match game.overlay {
        Overlay::Scoreboard => cmds.extend(scoreboard_overlay(game, layout)),
        Overlay::NameEntry => cmds.extend(name_entry_overlay(game, layout)),
        Overlay::About => cmds.extend(about_overlay(game, layout)),
        Overlay::None => {}
    }

    // The window's own raised bevel goes last so nothing covers it.  The
    // layout reserves `FRAME_RESERVE` rows below the board for exactly this.
    cmds.extend(client_frame_commands(game, layout));
    cmds
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
    let set = game.engine().use_figures() as u8;
    let scale = game.sheet_scale;
    let mut cmds = Vec::new();

    // Hide the source ball (the engine has not moved it yet).
    let (sr, sc) = rowline_core::coords(anim.path[0]);
    let (sx, sy) = layout.cell_origin(sr as i32, sc as i32);
    cmds.push(RenderCmd::Sprite {
        sheet: rowline_core::render::SpriteSheetId::MAIN,
        src: rowline_core::view::ball_src(0, set, scale),
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

    cmds.push(RenderCmd::Sprite {
        sheet: rowline_core::render::SpriteSheetId::MAIN,
        src: rowline_core::view::ball_inner_src(anim.colour, set, scale),
        dst: Point::new(cx - 15.0, cy - 15.0),
        size: Size::new(30.0, 30.0),
        tint: rowline_core::render::Color::WHITE,
    });
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
            src: rowline_core::view::ball_src(
                0,
                game.engine().use_figures() as u8,
                game.sheet_scale,
            ),
            dst: Point::new(x as f32, y as f32),
            size: Size::new(36.0, 36.0),
            tint: rowline_core::render::Color::WHITE,
        });
        cmds.push(RenderCmd::Sprite {
            sheet: rowline_core::render::SpriteSheetId::MAIN,
            src: rowline_core::view::ball_inner_src(
                colour,
                game.engine().use_figures() as u8,
                game.sheet_scale,
            ),
            dst: Point::new(
                x as f32 + rowline_core::view::BALL_INSET as f32,
                y as f32 + rowline_core::view::BALL_INSET as f32 - hop,
            ),
            size: Size::new(
                rowline_core::view::BALL_PX as f32,
                rowline_core::view::BALL_PX as f32,
            ),
            tint: rowline_core::render::Color::WHITE,
        });
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
        || game.help_open
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
    if game.design.motion.reduced {
        return Vec::new();
    }
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
                (i + j) % 2 == 0
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

/// The command reference overlay (the original's "Contents").
fn help_overlay(game: &Game, layout: Layout) -> Vec<RenderCmd> {
    let mut cmds = Vec::new();
    let ds = &game.design;
    let p = &ds.palette;

    let w = 360.0_f32.min(layout.client_w as f32 - 32.0).max(160.0);
    let rows = crate::commands::ALL.len() as f32;
    let h = (rows + 4.0) * 26.0 + 32.0;
    let rect = rowline_core::render::Rect::new(
        ((layout.client_w as f32 - w) * 0.5).max(8.0),
        ((layout.client_h as f32 - h) * 0.5).max(8.0),
        w,
        h,
    );
    cmds.push(RenderCmd::Shadow {
        rect,
        radius: ds.radius(rowline_theme::Radius::Lg),
        offset: rowline_core::input::Point::new(0.0, 8.0),
        blur: 32.0,
        color: p.shadow.into(),
    });
    cmds.push(RenderCmd::RoundedRect {
        rect,
        radius: ds.radius(rowline_theme::Radius::Lg),
        color: p.surface_raised.into(),
    });
    cmds.push(RenderCmd::Outline {
        rect,
        color: p.outline.into(),
    });
    cmds.push(RenderCmd::Text {
        text: "\u{5185}\u{5bb9}".to_string(),
        at: rowline_core::input::Point::new(rect.x + 16.0, rect.y + 12.0),
        size: ds.text_size(rowline_theme::TextRole::Title) as u16,
        color: p.text_primary.into(),
    });

    for (i, cmd) in crate::commands::ALL.iter().enumerate() {
        let y = rect.y + 44.0 + i as f32 * 26.0;
        if let Some(icon) = crate::commands::icon_for(*cmd) {
            cmds.push(RenderCmd::Icon {
                index: icon,
                dst: rowline_core::input::Point::new(rect.x + 16.0, y - 1.0),
                size: rowline_core::input::Size::new(16.0, 16.0),
                tint: p.text_secondary.into(),
            });
        }
        cmds.push(RenderCmd::Text {
            text: format!("{}  {}", cmd.label(), cmd.shortcut()),
            at: rowline_core::input::Point::new(rect.x + 40.0, y),
            size: ds.text_size(rowline_theme::TextRole::Body) as u16,
            color: p.text_secondary.into(),
        });
    }
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
        // The board strip and the cell plates are semantic tokens, so the
        // light theme gets its own warm-paper board instead of reusing the
        // white page surface.
        board_backdrop: p.board_backdrop.into(),
        cell_fill: p.board_cell.into(),
        cell_border: p.outline_strong.into(),
        cell_radius: game.design.radius(rowline_theme::Radius::Sm),
        selection: p.selection.into(),
        path: p.accent.into(),
        frame_light: rowline_core::render::Color::WHITE,
        frame_dark: rowline_core::render::Color::rgb(128, 128, 128),
        // The original is a Win95 window: every surface sits in a raised
        // bevel.  The modern themes stay flat.
        frame_thickness: if game.design.is_classic() { 2.0 } else { 0.0 },
    }
}

/// The raised bevel around the whole client area.
///
/// The original's window frame is drawn by Win32 outside the client; drawn
/// here it gives the same "boxed" look on every platform.
fn client_frame_commands(game: &Game, layout: Layout) -> Vec<RenderCmd> {
    if !game.design.is_classic() || layout.client_w < 8 || layout.client_h < 8 {
        return Vec::new();
    }
    let w = layout.client_w as f32;
    let h = layout.client_h as f32;
    let mut cmds = Vec::new();
    for i in 0..2 {
        let i = i as f32;
        cmds.push(RenderCmd::Frame {
            rect: rowline_core::render::Rect::new(i, i, w - 2.0 * i, h - 2.0 * i),
            style: FrameStyle::Raised,
        });
    }
    // The original has a one-pixel gap between the frame and the content.
    cmds.push(RenderCmd::Frame {
        rect: rowline_core::render::Rect::new(2.0, 2.0, w - 4.0, h - 4.0),
        style: FrameStyle::Flat,
    });
    cmds
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

    // The black readout, with the original's white frame around it.
    let score_box = |cmds: &mut Vec<RenderCmd>,
                     rect: rowline_core::render::Rect,
                     score: u32,
                     highlight: bool| {
        cmds.push(RenderCmd::RoundedRect {
            rect,
            radius: ds.radius(rowline_theme::Radius::Sm),
            color: p.score_fill.into(),
        });
        cmds.push(RenderCmd::Outline {
            rect,
            color: if highlight {
                rowline_core::render::Color::rgb(255, 210, 74)
            } else {
                p.score_border.into()
            },
        });
        if highlight {
            // A second, outer gold ring for the new-record flash.
            cmds.push(RenderCmd::Outline {
                rect: rowline_core::render::Rect::new(
                    rect.x - 1.0,
                    rect.y - 1.0,
                    rect.width + 2.0,
                    rect.height + 2.0,
                ),
                color: rowline_core::render::Color::rgb(255, 210, 74),
            });
        }
        let text = format!("{score}");
        // Right-aligned, as in the original.  Digits at this size are about
        // eight logical pixels wide.
        let width = 8.0 * text.chars().count() as f32;
        cmds.push(RenderCmd::Text {
            text,
            at: rowline_core::input::Point::new(rect.x + rect.width - 6.0 - width, rect.y + 3.0),
            size: text_size,
            color: p.score_text.into(),
        });
    };

    let box_y = y + (HUD_H - SCORE_BOX_H) * 0.5;
    score_box(
        &mut cmds,
        rowline_core::render::Rect::new(16.0, box_y, SCORE_BOX_W, SCORE_BOX_H),
        game.displayed_king_score(),
        game.record_celebrating(),
    );

    // ---- the next group (three cells, exactly 3 x 36 px wide) ----------
    //
    // 0x40B078: preview group x = (clientW - 0x6C) / 2.
    let group_x = ((w - PREVIEW_W) * 0.5).max(0.0);
    let cell_y = y + (HUD_H - 36.0) * 0.5;
    cmds.push(RenderCmd::Text {
        text: "\u{4e0b}\u{7ec4}".to_string(),
        at: rowline_core::input::Point::new(group_x - 36.0, y + 12.0),
        size: label_size,
        color: p.text_tertiary.into(),
    });
    for (i, colour) in game.engine().next_group().iter().enumerate() {
        let cx = group_x + i as f32 * 36.0;
        let cell = rowline_core::render::Rect::new(cx, cell_y, 36.0, 36.0);
        // Every slot is a beveled plate, even when it is empty.
        cmds.push(RenderCmd::Sprite {
            sheet: rowline_core::render::SpriteSheetId::MAIN,
            src: rowline_core::view::ball_src(
                0,
                game.engine().use_figures() as u8,
                game.sheet_scale,
            ),
            dst: rowline_core::input::Point::new(cell.x, cell.y),
            size: rowline_core::input::Size::new(36.0, 36.0),
            tint: rowline_core::render::Color::WHITE,
        });
        if *colour != 0 {
            cmds.push(RenderCmd::Sprite {
                sheet: rowline_core::render::SpriteSheetId::MAIN,
                src: rowline_core::view::ball_inner_src(
                    *colour,
                    game.engine().use_figures() as u8,
                    game.sheet_scale,
                ),
                // The preview balls are 24 px in the original, smaller than
                // the board balls.
                dst: rowline_core::input::Point::new(cell.x + 6.0, cell.y + 6.0),
                size: rowline_core::input::Size::new(24.0, 24.0),
                tint: rowline_core::render::Color::WHITE,
            });
        }
    }

    // ---- the colour / shape indicator ----------------------------------
    cmds.push(RenderCmd::Text {
        text: if game.engine().use_figures() {
            "\u{5f62}\u{72b6}".to_string()
        } else {
            "\u{989c}\u{8272}".to_string()
        },
        at: rowline_core::input::Point::new(group_x + PREVIEW_W + 8.0, y + 12.0),
        size: label_size,
        color: p.text_tertiary.into(),
    });

    // ---- the right score box -------------------------------------------
    score_box(
        &mut cmds,
        rowline_core::render::Rect::new(w - 16.0 - SCORE_BOX_W, box_y, SCORE_BOX_W, SCORE_BOX_H),
        game.challenger_score(),
        false,
    );

    cmds
}

/// The width of the statistics panel, in logical pixels.
///
/// The original's `XGrdCptDialog` client area is 154x289 (8 rows of 36 px);
/// the re-implementation adds a 22 px title bar.
const STATS_W: f32 = 154.0;
/// The height of the statistics title bar.
const STATS_TITLE_H: f32 = 22.0;
/// The height of one statistics row -- one sprite slot plus its caption.
const STATS_ROW_H: f32 = 36.0;

/// Draw the statistics window.
///
/// Recovered from the live original (`XGrdCptDialog`, title `统计`,
/// `0x404120`): eight rows, one per sprite slot.  Row 0 is the empty cell
/// count; rows 1..=7 are the seven ball colours.  Every row reads
/// `count (percent%)`; the ball rows append `,消除-N`, where `N` is the
/// cumulative number of balls of that colour eliminated this game.
fn stats_panel(game: &Game, layout: Layout) -> Vec<RenderCmd> {
    let mut cmds = Vec::new();
    let ds = &game.design;
    let p = &ds.palette;

    let height = STATS_TITLE_H + 8.0 * STATS_ROW_H;
    let rect = rowline_core::render::Rect::new(
        (layout.client_w as f32 - STATS_W - 2.0).max(0.0),
        layout.top as f32 + HUD_H + 2.0,
        STATS_W,
        height,
    );
    cmds.push(RenderCmd::Shadow {
        rect,
        radius: ds.radius(rowline_theme::Radius::Sm),
        offset: rowline_core::input::Point::new(0.0, 6.0),
        blur: 24.0,
        color: p.shadow.into(),
    });
    cmds.push(RenderCmd::RoundedRect {
        rect,
        radius: ds.radius(rowline_theme::Radius::Sm),
        color: p.surface_raised.into(),
    });
    cmds.push(RenderCmd::Outline {
        rect,
        color: p.outline.into(),
    });

    // The title bar.
    cmds.push(RenderCmd::RoundedRect {
        rect: rowline_core::render::Rect::new(rect.x, rect.y, rect.width, STATS_TITLE_H),
        radius: ds.radius(rowline_theme::Radius::Sm),
        color: p.surface_alt.into(),
    });
    cmds.push(RenderCmd::Text {
        text: "\u{7edf}\u{8ba1}".to_string(),
        at: rowline_core::input::Point::new(rect.x + 8.0, rect.y + 4.0),
        size: ds.text_size(rowline_theme::TextRole::Label) as u16,
        color: p.text_primary.into(),
    });
    cmds.push(RenderCmd::Text {
        text: "\u{00d7}".to_string(),
        at: rowline_core::input::Point::new(rect.x + rect.width - 16.0, rect.y + 3.0),
        size: ds.text_size(rowline_theme::TextRole::Body) as u16,
        color: p.text_tertiary.into(),
    });

    let counts = game.engine().colour_counts();
    let cleared = game.engine().cleared_counts();
    let total = rowline_core::board::BOARD_CELLS as u64;

    for slot in 0..8usize {
        let y = rect.y + STATS_TITLE_H + slot as f32 * STATS_ROW_H;
        // The sprite slot: a light plate with a 3-D border, as in the
        // original's `XGrdCpt` control.
        let plate = rowline_core::render::Rect::new(rect.x, y, STATS_ROW_H, STATS_ROW_H);
        cmds.push(RenderCmd::RoundedRect {
            rect: plate,
            radius: 0.0,
            color: p.board_cell.into(),
        });
        cmds.push(RenderCmd::Outline {
            rect: plate,
            color: p.outline_strong.into(),
        });
        if slot != 0 {
            cmds.push(RenderCmd::Sprite {
                sheet: rowline_core::render::SpriteSheetId::MAIN,
                src: rowline_core::view::ball_src(
                    slot as u8,
                    game.engine().use_figures() as u8,
                    game.sheet_scale,
                ),
                dst: rowline_core::input::Point::new(plate.x + 3.0, plate.y + 3.0),
                size: rowline_core::input::Size::new(30.0, 30.0),
                tint: rowline_core::render::Color::WHITE,
            });
        }

        let count = counts[slot];
        let percent = count as u64 * 100 / total;
        let mut text = format!("{count} ({percent}%)");
        if slot != 0 {
            text.push_str(&format!(",\u{6d88}\u{9664}-{}", cleared[slot]));
        }
        cmds.push(RenderCmd::Text {
            text,
            at: rowline_core::input::Point::new(rect.x + 44.0, y + 9.0),
            size: ds.text_size(rowline_theme::TextRole::Body) as u16,
            color: p.text_primary.into(),
        });
    }

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
    cmds.push(RenderCmd::Shadow {
        rect,
        radius: ds.radius(rowline_theme::Radius::Lg),
        offset: rowline_core::input::Point::new(0.0, 8.0),
        blur: 32.0,
        color: p.shadow.into(),
    });
    cmds.push(RenderCmd::RoundedRect {
        rect,
        radius: ds.radius(rowline_theme::Radius::Lg),
        color: p.surface_raised.into(),
    });
    cmds.push(RenderCmd::Outline {
        rect,
        color: p.outline.into(),
    });

    // The title bar of the dialog.
    cmds.push(RenderCmd::RoundedRect {
        rect: rowline_core::render::Rect::new(rect.x, rect.y, rect.width, 32.0),
        radius: ds.radius(rowline_theme::Radius::Lg),
        color: p.surface_alt.into(),
    });
    cmds.push(RenderCmd::Text {
        text: format!(
            "{} {}",
            rowline_core::PRODUCT_NAME,
            env!("CARGO_PKG_VERSION")
        ),
        at: rowline_core::input::Point::new(rect.x + 12.0, rect.y + 8.0),
        size: ds.text_size(rowline_theme::TextRole::Label) as u16,
        color: p.text_primary.into(),
    });

    // The warning mark: the original's yellow warning triangle reduced to a
    // trio of stacked bars (a diamond is not available as a primitive).
    let mark = rowline_core::render::Rect::new(rect.x + 24.0, rect.y + 52.0, 40.0, 40.0);
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
        text: "\u{6e38}\u{620f}\u{7ed3}\u{675f}!".to_string(),
        at: rowline_core::input::Point::new(rect.x + 80.0, rect.y + 62.0),
        size: ds.text_size(rowline_theme::TextRole::Title) as u16,
        color: p.text_primary.into(),
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
        color: p.surface_raised.into(),
    });
    cmds.push(RenderCmd::Outline {
        rect: btn,
        color: p.outline_strong.into(),
    });
    cmds.push(RenderCmd::Text {
        text: "\u{786e}\u{5b9a}(O)".to_string(),
        at: rowline_core::input::Point::new(btn.x + 48.0, btn.y + 9.0),
        size: ds.text_size(rowline_theme::TextRole::Body) as u16,
        color: p.text_primary.into(),
    });
    cmds
}

/// Draw the about box.
fn about_overlay(game: &Game, layout: Layout) -> Vec<RenderCmd> {
    let mut cmds = Vec::new();
    let ds = &game.design;
    let p = &ds.palette;
    let w = 380.0_f32.min(layout.client_w as f32 - 32.0).max(240.0);
    let h = 190.0;
    let rect = rowline_core::render::Rect::new(
        ((layout.client_w as f32 - w) * 0.5).max(8.0),
        ((layout.client_h as f32 - h) * 0.5).max(8.0),
        w,
        h,
    );
    cmds.push(RenderCmd::Shadow {
        rect,
        radius: ds.radius(rowline_theme::Radius::Sm),
        offset: rowline_core::input::Point::new(0.0, 6.0),
        blur: 24.0,
        color: p.shadow.into(),
    });
    cmds.push(RenderCmd::RoundedRect {
        rect,
        radius: ds.radius(rowline_theme::Radius::Sm),
        color: p.surface_raised.into(),
    });
    cmds.push(RenderCmd::Outline {
        rect,
        color: p.outline.into(),
    });
    let about = [
        format!(
            "{} {}",
            rowline_core::PRODUCT_NAME,
            env!("CARGO_PKG_VERSION")
        ),
        String::new(),
        "An independent re-implementation of the".to_string(),
        "behaviour of WinLinez (c) 1998-1999 Ivan Golubev.".to_string(),
        "No original code or artwork is used.".to_string(),
        String::new(),
        "Built with Rust, winit and wgpu.".to_string(),
    ];
    for (i, line) in about.iter().enumerate() {
        cmds.push(RenderCmd::Text {
            text: (*line).to_string(),
            at: rowline_core::input::Point::new(rect.x + 18.0, rect.y + 40.0 + i as f32 * 20.0),
            size: ds.text_size(rowline_theme::TextRole::Body) as u16,
            color: p.text_primary.into(),
        });
    }
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
    if game.design.motion.reduced {
        return 0.0;
    }
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
    let idle = if game.design.motion.reduced {
        0.0
    } else {
        (game.clock_seconds() * 2.4 + phase).sin() * 1.2
    };
    let feet = feet + idle + lift;

    // ---- the pedestal ---------------------------------------------------
    let column = rowline_core::render::Rect::new(
        centre - PEDESTAL_W / 2.0,
        feet + 1.0,
        PEDESTAL_W,
        (pedestal_bottom - 4.0 - feet - 1.0).max(4.0),
    );
    let (column_from, column_to) = if ds.is_classic() {
        (
            rowline_core::render::Color::rgb(255, 255, 255),
            rowline_core::render::Color::rgb(128, 128, 128),
        )
    } else {
        (p.surface_raised.into(), p.surface_alt.into())
    };
    cmds.push(RenderCmd::Gradient {
        rect: column,
        radius: if ds.is_classic() {
            2.0
        } else {
            PEDESTAL_W / 2.0
        },
        from: column_from,
        to: column_to,
        direction: rowline_core::render::GradientDirection::Horizontal,
    });
    cmds.push(RenderCmd::Outline {
        rect: column,
        color: p.outline.into(),
    });
    // The cap under the figure's feet.
    cmds.push(RenderCmd::RoundedRect {
        rect: rowline_core::render::Rect::new(
            centre - PEDESTAL_W / 2.0 - 2.0,
            feet - 3.0,
            PEDESTAL_W + 4.0,
            6.0,
        ),
        radius: 2.0,
        color: rowline_core::render::Color::rgb(224, 224, 224),
    });
    // The base plate.
    let foot = rowline_core::render::Rect::new(
        centre - PEDESTAL_FOOT_W / 2.0,
        pedestal_bottom - PEDESTAL_FOOT_H,
        PEDESTAL_FOOT_W,
        PEDESTAL_FOOT_H,
    );
    cmds.push(RenderCmd::RoundedRect {
        rect: foot,
        radius: if ds.is_classic() { 2.0 } else { 6.0 },
        color: rowline_core::render::Color::rgb(224, 224, 224),
    });
    cmds.push(RenderCmd::Outline {
        rect: foot,
        color: p.outline.into(),
    });

    // ---- the character --------------------------------------------------
    // Saturated bodies with a dark rim: on the black board a pale pastel
    // body washed out, and the rim keeps the silhouette readable.
    let (body_colour, belly_colour, rim_colour) = match figure {
        Figure::King => (
            rowline_core::render::Color::rgb(176, 16, 16),
            rowline_core::render::Color::rgb(224, 90, 90),
            rowline_core::render::Color::rgb(82, 0, 0),
        ),
        Figure::Challenger => (
            rowline_core::render::Color::rgb(160, 0, 160),
            rowline_core::render::Color::rgb(222, 96, 222),
            rowline_core::render::Color::rgb(72, 0, 72),
        ),
    };
    let gold = rowline_core::render::Color::rgb(255, 210, 74);
    let dark = rowline_core::render::Color::rgb(48, 16, 16);
    let white = rowline_core::render::Color::WHITE;

    // Arms first, so the body overlaps their inner ends.
    for dx in [-24.0_f32, 17.0] {
        cmds.push(RenderCmd::RoundedRect {
            rect: rowline_core::render::Rect::new(centre + dx, feet - 34.0, 9.0, 18.0),
            radius: 4.5,
            color: body_colour,
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
    cmds.push(RenderCmd::RoundedRect {
        rect: body_rect,
        radius: 18.0,
        color: body_colour,
    });
    // Belly.
    cmds.push(RenderCmd::RoundedRect {
        rect: rowline_core::render::Rect::new(centre - 13.0, feet - 29.0, 26.0, 22.0),
        radius: 12.0,
        color: belly_colour,
    });
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
    cmds.push(RenderCmd::RoundedRect {
        rect: head_rect,
        radius: 15.0,
        color: body_colour,
    });
    // Face patch.
    cmds.push(RenderCmd::RoundedRect {
        rect: rowline_core::render::Rect::new(centre - 12.0, head_y + 8.0, 24.0, 18.0),
        radius: 9.0,
        color: belly_colour,
    });
    // Eyes.  They blink for a tenth of a second every few seconds.
    let blink = !game.design.motion.reduced && (game.clock_seconds() + phase) % 3.7 < 0.12;
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

    // ---- the crown (both figures wear one in the original) --------------
    cmds.push(RenderCmd::RoundedRect {
        rect: rowline_core::render::Rect::new(centre - 13.0, head_y - 5.0, 26.0, 7.0),
        radius: 2.5,
        color: gold,
    });
    for (dx, h) in [(-10.0_f32, 7.0_f32), (-2.0, 10.0), (6.0, 7.0)] {
        cmds.push(RenderCmd::RoundedRect {
            rect: rowline_core::render::Rect::new(centre + dx, head_y - 5.0 - h, 4.5, h + 2.0),
            radius: 1.5,
            color: gold,
        });
    }

    // ---- the emblem: sceptre for the CPU, sword for the challenger ------
    match figure {
        Figure::King => {
            // A sceptre held in the right hand.
            cmds.push(RenderCmd::RoundedRect {
                rect: rowline_core::render::Rect::new(centre + 22.0, feet - 58.0, 3.0, 34.0),
                radius: 1.5,
                color: gold,
            });
            cmds.push(RenderCmd::RoundedRect {
                rect: rowline_core::render::Rect::new(centre + 19.0, feet - 67.0, 9.0, 9.0),
                radius: 4.5,
                color: gold,
            });
        }
        Figure::Challenger => {
            cmds.push(RenderCmd::Icon {
                index: crate::commands::ICON_SWORD,
                dst: rowline_core::input::Point::new(centre + 18.0, feet - 70.0),
                size: rowline_core::input::Size::new(24.0, 36.0),
                tint: rowline_core::render::Color::rgb(240, 240, 240),
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
        color: if ds.is_classic() {
            p.text_on_accent.into()
        } else {
            p.text_secondary.into()
        },
    });

    cmds
}

/// The leaderboard geometry, shared by drawing and hit-testing.
fn scoreboard_rects(layout: Layout, classic: bool) -> (rowline_core::render::Rect, f32) {
    // The original's leaderboard is a separate dialog that can be taller
    // than the game window.  The re-implementation draws it as an overlay,
    // so the row height shrinks to keep all sixteen rows on screen at the
    // original's small default client size.
    let content_h = (layout.client_h - layout.top).max(1) as f32;
    let margin = 4.0;
    let base_w: f32 = if classic { 360.0 } else { 420.0 };
    let w = base_w.min(layout.client_w as f32 - 32.0).max(240.0);
    // The panel fills the play area below the HUD, so the preview cells are
    // not left half-covered above it.
    let h = (content_h - 2.0 * margin).max(120.0);
    let row_h = ((h - 80.0) / 16.0).max(8.0);
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
    let h = 150.0;
    let rect = rowline_core::render::Rect::new(
        ((layout.client_w as f32 - w) * 0.5).max(8.0),
        ((layout.client_h as f32 - h) * 0.5).max(8.0),
        w,
        h,
    );
    let field =
        rowline_core::render::Rect::new(rect.x + 16.0, rect.y + 72.0, rect.width - 32.0, 30.0);
    let ok = rowline_core::render::Rect::new(
        rect.x + (rect.width - 120.0) * 0.5,
        rect.y + rect.height - 44.0,
        120.0,
        30.0,
    );
    (rect, field, ok)
}

/// Draw the leaderboard dialog (总榜).
fn scoreboard_overlay(game: &Game, layout: Layout) -> Vec<RenderCmd> {
    let mut cmds = Vec::new();
    let ds = &game.design;
    let p = &ds.palette;
    let sb = rowline_core::scoreboard::Scoreboard::new(game.scores().to_vec());

    let (rect, row_h) = scoreboard_rects(layout, ds.is_classic());
    cmds.push(RenderCmd::Shadow {
        rect,
        radius: ds.radius(rowline_theme::Radius::Sm),
        offset: rowline_core::input::Point::new(0.0, 6.0),
        blur: 24.0,
        color: p.shadow.into(),
    });
    cmds.push(RenderCmd::RoundedRect {
        rect,
        radius: ds.radius(rowline_theme::Radius::Sm),
        color: p.surface_raised.into(),
    });
    cmds.push(RenderCmd::Outline {
        rect,
        color: p.outline.into(),
    });

    // The title.
    cmds.push(RenderCmd::RoundedRect {
        rect: rowline_core::render::Rect::new(rect.x, rect.y, rect.width, 28.0),
        radius: ds.radius(rowline_theme::Radius::Sm),
        color: p.surface_alt.into(),
    });
    cmds.push(RenderCmd::Text {
        text: "\u{603b}\u{699c}".to_string(),
        at: rowline_core::input::Point::new(rect.x + 12.0, rect.y + 7.0),
        size: ds.text_size(rowline_theme::TextRole::Label) as u16,
        color: p.text_primary.into(),
    });

    // The sixteen rows.  The rank/name column starts at a fixed x and the
    // score column is right-aligned, so mixed-width names (Latin + CJK)
    // cannot push the numbers out of line.
    let top = rect.y + 36.0;
    let row_size = ds.text_size(rowline_theme::TextRole::Body) as u16;
    let alt = if ds.is_classic() {
        rowline_core::render::Color::rgb(0xE4, 0xE4, 0xE4)
    } else {
        p.surface_alt.into()
    };
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

    // The two buttons: Clear and OK, centred as a pair inside the panel.
    let (clear, ok) = scoreboard_buttons((rect, row_h));
    // The original's buttons are 清除 and 确定(O).
    for (btn, label, colour) in [
        (clear, "\u{6e05}\u{9664}", p.danger),
        (ok, "\u{786e}\u{5b9a}(O)", p.accent),
    ] {
        cmds.push(RenderCmd::RoundedRect {
            rect: btn,
            radius: ds.radius(rowline_theme::Radius::Sm),
            color: p.surface_raised.into(),
        });
        cmds.push(RenderCmd::Outline {
            rect: btn,
            color: p.outline_strong.into(),
        });
        let text_w = 12.0 * label.chars().count() as f32;
        cmds.push(RenderCmd::Text {
            text: label.to_string(),
            at: rowline_core::input::Point::new(btn.x + (btn.width - text_w) * 0.5, btn.y + 6.0),
            size: ds.text_size(rowline_theme::TextRole::Body) as u16,
            color: colour.into(),
        });
    }
    cmds
}

/// Draw the name entry dialog (输入你的名字).
fn name_entry_overlay(game: &Game, layout: Layout) -> Vec<RenderCmd> {
    let mut cmds = Vec::new();
    let ds = &game.design;
    let p = &ds.palette;

    let (rect, field, ok) = name_entry_rects(layout);
    cmds.push(RenderCmd::Shadow {
        rect,
        radius: ds.radius(rowline_theme::Radius::Sm),
        offset: rowline_core::input::Point::new(0.0, 6.0),
        blur: 24.0,
        color: p.shadow.into(),
    });
    cmds.push(RenderCmd::RoundedRect {
        rect,
        radius: ds.radius(rowline_theme::Radius::Sm),
        color: p.surface_raised.into(),
    });
    cmds.push(RenderCmd::Outline {
        rect,
        color: p.outline.into(),
    });

    cmds.push(RenderCmd::Text {
        text: "\u{8f93}\u{5165}\u{4f60}\u{7684}\u{540d}\u{5b57}:".to_string(),
        at: rowline_core::input::Point::new(rect.x + 16.0, rect.y + 46.0),
        size: ds.text_size(rowline_theme::TextRole::Body) as u16,
        color: p.text_primary.into(),
    });

    // The edit field.  The original's Win95 edit control is white with
    // black text; using the theme's `surface` here made the black classic
    // text invisible on a black field.
    let (field_fill, field_text) = if ds.is_classic() {
        (
            rowline_core::render::Color::WHITE,
            rowline_core::render::Color::BLACK,
        )
    } else {
        (p.surface.into(), p.text_primary.into())
    };
    cmds.push(RenderCmd::RoundedRect {
        rect: field,
        radius: ds.radius(rowline_theme::Radius::Sm),
        color: field_fill,
    });
    cmds.push(RenderCmd::Outline {
        rect: field,
        color: p.outline_strong.into(),
    });
    cmds.push(RenderCmd::Text {
        text: game.name_input.clone(),
        at: rowline_core::input::Point::new(field.x + 8.0, field.y + 7.0),
        size: ds.text_size(rowline_theme::TextRole::Body) as u16,
        color: field_text,
    });

    // The OK button.
    cmds.push(RenderCmd::RoundedRect {
        rect: ok,
        radius: ds.radius(rowline_theme::Radius::Sm),
        color: p.surface_raised.into(),
    });
    cmds.push(RenderCmd::Outline {
        rect: ok,
        color: p.outline_strong.into(),
    });
    cmds.push(RenderCmd::Text {
        text: "\u{786e}\u{5b9a}(O)".to_string(),
        at: rowline_core::input::Point::new(ok.x + 48.0, ok.y + 7.0),
        size: ds.text_size(rowline_theme::TextRole::Body) as u16,
        color: p.text_primary.into(),
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
        radius: ds.radius(rowline_theme::Radius::Md),
        color: p.surface_alt.into(),
    });

    for (i, menu) in game.menu.menus.iter().enumerate() {
        let x = i as f32 * m.title_width;
        let rect = rowline_core::render::Rect::new(x, 0.0, m.title_width, m.bar_height);
        let open = game.menu.open == Some(i);
        if open {
            cmds.push(RenderCmd::RoundedRect {
                rect,
                radius: ds.radius(rowline_theme::Radius::Sm),
                color: p.surface_hover.into(),
            });
        }
        cmds.push(RenderCmd::Text {
            text: menu.title.to_string(),
            at: rowline_core::input::Point::new(
                x + ds.space(rowline_theme::Space::Xs),
                m.bar_height * 0.2,
            ),
            size: ds.text_size(rowline_theme::TextRole::Label) as u16,
            color: if open && ds.is_classic() {
                p.text_on_accent.into()
            } else if ds.is_classic() {
                p.text_primary.into()
            } else if open {
                p.accent.into()
            } else {
                p.text_secondary.into()
            },
        });
    }

    // ---- the toolbar ---------------------------------------------------
    //
    // The original has no toolbar: its window is title bar + native menu +
    // HUD.  The two shortcut buttons are a re-design, so the classic theme
    // hides them (Ctrl+T / Ctrl+R still work).
    if !ds.is_classic() {
        let tb = game.menu_toolbar_rect(layout.client_w);
        let icon = tb.height * 0.72;
        for (i, (index, tint)) in [
            (crate::commands::ICON_THEME, p.text_secondary),
            (crate::commands::ICON_SETTINGS, p.text_secondary),
        ]
        .iter()
        .enumerate()
        {
            let x = tb.x + i as f32 * (tb.width * 0.5) + 4.0;
            cmds.push(RenderCmd::Icon {
                index: *index,
                dst: rowline_core::input::Point::new(x, tb.y + (tb.height - icon) * 0.5),
                size: rowline_core::input::Size::new(icon, icon),
                tint: (*tint).into(),
            });
        }
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
        if item.is_separator() {
            cmds.push(RenderCmd::Outline {
                rect: rowline_core::render::Rect::new(
                    panel.x + m.pad,
                    y + m.row_height * 0.5,
                    panel.width - m.pad * 2.0,
                    1.0,
                ),
                color: p.outline.into(),
            });
            continue;
        }
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
        // The command's icon, if it has one.  Classic Win95 menus are text
        // only, exactly like the original's RT_MENU.
        let mut label_x = panel.x + m.pad;
        if !ds.is_classic() {
            if let Some(icon) =
                crate::commands::icon_for(crate::commands::Command(item.id.unwrap_or(0)))
            {
                let side = m.row_height - 8.0;
                cmds.push(RenderCmd::Icon {
                    index: icon,
                    dst: rowline_core::input::Point::new(panel.x + m.pad, y + 4.0),
                    size: rowline_core::input::Size::new(side, side),
                    tint: if hovered {
                        p.accent.into()
                    } else {
                        p.text_secondary.into()
                    },
                });
            }
            label_x += 28.0;
        }
        cmds.push(RenderCmd::Text {
            text: item.label.to_string(),
            at: rowline_core::input::Point::new(label_x, y + m.row_height * 0.2),
            size: ds.text_size(rowline_theme::TextRole::Body) as u16,
            color: if hovered && ds.is_classic() {
                p.text_on_accent.into()
            } else if hovered {
                p.text_primary.into()
            } else {
                p.text_secondary.into()
            },
        });
        if !item.hint.is_empty() {
            cmds.push(RenderCmd::Text {
                text: item.hint.to_string(),
                at: rowline_core::input::Point::new(
                    panel.x + panel.width - m.pad - 48.0,
                    y + m.row_height * 0.2,
                ),
                size: ds.text_size(rowline_theme::TextRole::Caption) as u16,
                color: if hovered && ds.is_classic() {
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
    fn toggles_flip_the_settings() {
        let mut g = Game::new(1, RuleModel::default());
        let before = g.settings().show_next;
        g.run_command(Command::TOGGLE_NEXT_SET);
        assert_ne!(before, g.settings().show_next);

        let before = g.settings().use_figures;
        g.run_command(Command::TOGGLE_USE_FIGURES);
        assert_ne!(before, g.settings().use_figures);
        // the engine and the setting must stay in sync
        assert_eq!(g.engine().use_figures(), g.settings().use_figures);
    }

    #[test]
    fn the_hud_shows_three_preview_balls() {
        // The original previews the next group of three (docs/11 section 2.1).
        let g = Game::new(7, RuleModel::default());
        let layout = rowline_core::layout::Layout::compute(1280, 960);
        let cmds = frame(&g, layout);
        let sprites = cmds
            .iter()
            .filter(|c| matches!(c, RenderCmd::Sprite { .. }))
            .count();
        // three previews + the balls that are on the board
        assert!(
            sprites >= 3,
            "expected at least the three previews, got {sprites}"
        );
        let texts: Vec<&str> = cmds
            .iter()
            .filter_map(|c| match c {
                RenderCmd::Text { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect();
        assert!(
            texts.contains(&"\u{4e0b}\u{7ec4}"),
            "the next-group label is missing"
        );
        assert!(texts.contains(&"\u{989c}\u{8272}") || texts.contains(&"\u{5f62}\u{72b6}"));
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
        assert_eq!(plates, 84);
        // The classic default paints the board strip black, exactly like
        // the original.
        assert!(cmds.iter().any(|c| matches!(
            c,
            RenderCmd::RoundedRect { rect, color, .. }
                if rect.x == 0.0
                    && rect.y == layout.board_y as f32
                    && rect.width == layout.client_w as f32
                    && *color == rowline_core::render::Color::BLACK
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
        let start = g.name_input.len();

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
        assert!(g.name_input.ends_with("剑客"), "got {:?}", g.name_input);
        assert_eq!(g.name_input.len(), start + "剑客".len());
        assert!(!g.name_input.contains("jian"));
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
        let (panel, _) = scoreboard_rects(layout, true);
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
        let (panel, row_h) = scoreboard_rects(layout, true);
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
    fn reduced_motion_moves_without_an_animation() {
        let mut g = Game::new(7, RuleModel::default());
        g.toggle_reduced_motion();
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
    fn the_classic_name_field_is_white_with_black_text() {
        let layout = Layout::compute_framed(
            604,
            396,
            rowline_core::layout::MENU_BAR_H,
            rowline_core::layout::FRAME_RESERVE,
        );
        let mut g = Game::new(7, RuleModel::default());
        g.offer_name_entry();
        let cmds = frame(&g, layout);
        let (_panel, field, _ok) = name_entry_rects(layout);
        assert!(
            cmds.iter().any(|c| matches!(
                c,
                RenderCmd::RoundedRect { rect, color, .. }
                    if *rect == field
                        && *color == rowline_core::render::Color::WHITE
            )),
            "the classic edit field must be white"
        );
        assert!(
            cmds.iter().any(|c| matches!(
                c,
                RenderCmd::Text { text, color, .. }
                    if text == &g.name_input
                        && *color == rowline_core::render::Color::BLACK
            )),
            "the name must be drawn in black on the white field"
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
        g.menu_click(Point::new(30.0, 10.0), 604);
        assert_eq!(g.menu.open, Some(0));
        // Once open, moving across the bar switches menus.
        g.menu_hover(Point::new(100.0, 10.0));
        assert_eq!(g.menu.open, Some(1));
    }

    #[test]
    fn the_hidden_classic_toolbar_does_not_toggle_anything() {
        let mut g = Game::new(7, RuleModel::default());
        assert!(g.design.is_classic());
        let retro = g.settings().retro_mode;
        let theme = g.design.kind();
        // The top-right spot is where the re-design's two shortcut buttons
        // live; the original has none, so a click there must not toggle the
        // theme or the display policy.
        g.menu_click(Point::new(590.0, 5.0), 604);
        assert_eq!(g.settings().retro_mode, retro);
        assert_eq!(g.design.kind(), theme);
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
        // The original ships with ShowStatistics=0.
        assert!(!g.settings().show_statistics);
        let texts = |cmds: &[RenderCmd]| -> Vec<String> {
            cmds.iter()
                .filter_map(|c| match c {
                    RenderCmd::Text { text, .. } => Some(text.clone()),
                    _ => None,
                })
                .collect()
        };
        assert!(!texts(&frame(&g, layout)).iter().any(|t| t == "统计"));

        g.settings.show_statistics = true;
        {
            let b = g.engine_mut().board_mut();
            b.cells_mut().fill(0);
            b.set(0, 0, 1);
        }
        let cmds = frame(&g, layout);
        let texts = texts(&cmds);
        assert!(texts.iter().any(|t| t == "统计"));
        // 80 empty cells of 81 -> 98%, one green ball -> 1%.
        assert!(
            texts.iter().any(|t| t == "80 (98%)"),
            "empty row missing: {texts:?}"
        );
        assert!(
            texts.iter().any(|t| t == "1 (1%),消除-0"),
            "colour row missing: {texts:?}"
        );
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
            texts.contains(&"\u{6e38}\u{620f}\u{7ed3}\u{675f}!"),
            "the dialog text is missing"
        );
        assert!(
            texts.contains(&"\u{786e}\u{5b9a}(O)"),
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
        g.name_key(rowline_core::input::Key::KeyA);
        assert_eq!(g.name_input, "挑战者a");
        g.name_key(rowline_core::input::Key::Backspace);
        assert_eq!(g.name_input, "挑战者");
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
        g.toggle_reduced_motion();
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
