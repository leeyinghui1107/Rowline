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
use rowline_core::{RuleModel, ScoreRecord, Settings};

use rowline_theme::{DesignSystem, ThemeKind};

use crate::commands::Command;

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
        Game {
            engine,
            settings: Settings::default(),
            scores: rowline_core::scoreboard::default_table(),
            selected: None,
            path: Vec::new(),
            phase: Phase::Ready,
            quit: false,
            design: DesignSystem::new(ThemeKind::Dark, false),
            menu: rowline_core::menu::MenuModel::new(rowline_core::menu::default_menus()),
            effects: rowline_core::effects::EffectSystem::new(0x5EED),
            help_open: false,
            game_over: false,
            game_over_ack: true,
            overlay: Overlay::None,
            name_input: rowline_core::scoreboard::default_player_name().to_string(),
            viewport_dirty: false,
            sheet_scale: 1,
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
    pub fn king_score(&self) -> u32 {
        rowline_core::scoreboard::king_score_in(&self.scores)
    }

    /// The human's score -- the HUD's **right** score box.
    pub fn challenger_score(&self) -> u32 {
        self.engine.score() as u32
    }

    /// Advance the decorative effects.
    pub fn tick(&mut self, dt: f32) {
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

    /// The score table.
    #[allow(dead_code)]
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
        self.design.set_theme(next);
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
        let theme = if self.settings.theme == 1 {
            ThemeKind::Light
        } else {
            ThemeKind::Dark
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
        self.apply_settings();
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
            InputEvent::KeyPressed { key, .. } => {
                // The name entry captures the typing.
                if self.overlay == Overlay::NameEntry {
                    self.name_key(*key);
                    return;
                }
                // The menu has priority over the game commands.
                if self.menu_key(*key) {
                    return;
                }
                if let Some(cmd) = Command::from_key(*key) {
                    self.run_command(cmd);
                }
            }
            InputEvent::MousePressed { pos, button, .. } => {
                if *button == MouseButton::Left {
                    // The menu has priority over the board.
                    if self.menu_click(*pos, layout.client_w) {
                        return;
                    }
                    self.on_click(*pos, layout);
                }
            }
            InputEvent::MouseMoved { pos } => {
                self.menu_hover(*pos);
            }
            InputEvent::QuitRequested => self.quit = true,
            _ => {}
        }
    }

    /// Execute a menu command.
    pub fn run_command(&mut self, cmd: Command) {
        match cmd {
            Command::NEW_GAME => {
                self.engine_mut().new_game();
                self.selected = None;
                self.path.clear();
                self.phase = Phase::Ready;
                self.game_over = false;
                self.game_over_ack = true;
                self.overlay = Overlay::None;
            }
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
        // The toolbar, at the right of the menu bar.
        if let Some(btn) = self.toolbar_button_at(pos, client_w) {
            match btn {
                0 => self.toggle_theme(),
                _ => self.toggle_retro_mode(),
            }
            return true;
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

    fn on_click(&mut self, pos: Point, layout: Layout) {
        let Some((r, c)) = layout.hit_cell(pos.x as i32, pos.y as i32) else {
            return;
        };
        let cell = rowline_core::index(r as usize, c as usize);
        match self.selected {
            None => {
                if self.engine.board().colour_at(r as usize, c as usize) != 0 {
                    self.selected = Some(cell);
                    self.phase = Phase::Selected;
                }
            }
            Some(prev) => {
                if prev == cell {
                    self.selected = None;
                    self.phase = Phase::Ready;
                    return;
                }
                let (pr, pc) = rowline_core::coords(prev);
                let res = self.engine.move_ball(pr as i32, pc as i32, r, c);
                match res.status {
                    Some(rowline_core::MoveStatus::Moved) => {
                        self.selected = None;
                        self.path = res.path.clone();
                        self.phase = if res.group.is_empty() {
                            Phase::Ready
                        } else {
                            Phase::Animating
                        };
                        if !self.engine.has_legal_move() {
                            self.game_over = true;
                            self.game_over_ack = false;
                        }
                        if !res.group.is_empty() {
                            for cell in &res.group {
                                let (gr, gc) = rowline_core::coords(*cell);
                                self.burst_at(gr, gc, layout);
                            }
                        }
                    }
                    _ => {
                        self.selected = None;
                        self.phase = Phase::Ready;
                    }
                }
            }
        }
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
    }
}

/// The height of the menu bar, in logical pixels.
const MENU_BAR_H: f32 = 32.0;
/// The height of the HUD bar, in logical pixels.
const HUD_H: f32 = 64.0;

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
    let y = MENU_BAR_H;
    let pad = ds.space(rowline_theme::Space::Lg);

    // The bar itself.
    cmds.push(RenderCmd::RoundedRect {
        rect: rowline_core::render::Rect::new(0.0, y, w, HUD_H),
        radius: 0.0,
        color: p.surface_alt.into(),
    });
    cmds.push(RenderCmd::Outline {
        rect: rowline_core::render::Rect::new(0.0, y + HUD_H - 1.0, w, 1.0),
        color: p.outline.into(),
    });

    let box_h = 40.0;
    let box_y = y + (HUD_H - box_h) * 0.5;

    // ---- the left score box -------------------------------------------
    let left_box = rowline_core::render::Rect::new(pad, box_y, 120.0, box_h);
    cmds.push(RenderCmd::RoundedRect {
        rect: left_box,
        radius: ds.radius(rowline_theme::Radius::Sm),
        color: p.surface_raised.into(),
    });
    cmds.push(RenderCmd::Outline {
        rect: left_box,
        color: p.outline.into(),
    });
    cmds.push(RenderCmd::Text {
        text: format!("{}", game.king_score()),
        at: rowline_core::input::Point::new(left_box.x + 12.0, left_box.y + 10.0),
        size: ds.text_size(rowline_theme::TextRole::Numeric) as u16,
        color: p.text_primary.into(),
    });

    // ---- the next group ------------------------------------------------
    let mut x = left_box.x + left_box.width + ds.space(rowline_theme::Space::Xl);
    cmds.push(RenderCmd::Text {
        text: "\u{4e0b}\u{7ec4}".to_string(),
        at: rowline_core::input::Point::new(x, y + HUD_H * 0.36),
        size: ds.text_size(rowline_theme::TextRole::Label) as u16,
        color: p.text_secondary.into(),
    });
    x += ds.space(rowline_theme::Space::Lg) + 40.0;
    for colour in game.engine().next_group() {
        let cell = rowline_core::render::Rect::new(x, box_y, 40.0, box_h);
        cmds.push(RenderCmd::RoundedRect {
            rect: cell,
            radius: ds.radius(rowline_theme::Radius::Sm),
            color: p.surface_raised.into(),
        });
        cmds.push(RenderCmd::Outline {
            rect: cell,
            color: p.outline.into(),
        });
        if colour != 0 {
            cmds.push(RenderCmd::Sprite {
                sheet: rowline_core::render::SpriteSheetId::MAIN,
                src: rowline_core::view::ball_src(
                    colour,
                    game.engine().use_figures() as u8,
                    game.sheet_scale,
                ),
                dst: rowline_core::input::Point::new(cell.x + 4.0, cell.y + 4.0),
                size: rowline_core::input::Size::new(32.0, 32.0),
                tint: rowline_core::render::Color::WHITE,
            });
        }
        x += cell.width + ds.space(rowline_theme::Space::Xs);
    }

    // ---- the colour / shape indicator ----------------------------------
    cmds.push(RenderCmd::Text {
        text: if game.engine().use_figures() {
            "\u{5f62}\u{72b6}".to_string()
        } else {
            "\u{989c}\u{8272}".to_string()
        },
        at: rowline_core::input::Point::new(x + 8.0, y + HUD_H * 0.36),
        size: ds.text_size(rowline_theme::TextRole::Label) as u16,
        color: p.text_secondary.into(),
    });

    // ---- the right score box -------------------------------------------
    let right_box = rowline_core::render::Rect::new(w - pad - 120.0, box_y, 120.0, box_h);
    cmds.push(RenderCmd::RoundedRect {
        rect: right_box,
        radius: ds.radius(rowline_theme::Radius::Sm),
        color: p.surface_raised.into(),
    });
    cmds.push(RenderCmd::Outline {
        rect: right_box,
        color: p.outline.into(),
    });
    cmds.push(RenderCmd::Text {
        text: format!("{}", game.challenger_score()),
        at: rowline_core::input::Point::new(right_box.x + 12.0, right_box.y + 10.0),
        size: ds.text_size(rowline_theme::TextRole::Numeric) as u16,
        color: p.text_primary.into(),
    });

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
        MENU_BAR_H + HUD_H + 2.0,
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

    // The warning mark: a pill with an exclamation point.
    let mark = rowline_core::render::Rect::new(rect.x + 24.0, rect.y + 52.0, 40.0, 40.0);
    cmds.push(RenderCmd::RoundedRect {
        rect: mark,
        radius: 20.0,
        color: p.warning.into(),
    });
    cmds.push(RenderCmd::Text {
        text: "!".to_string(),
        at: rowline_core::input::Point::new(mark.x + 14.0, mark.y + 8.0),
        size: ds.text_size(rowline_theme::TextRole::Title) as u16,
        color: p.text_on_accent.into(),
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
    cmds.extend(one_figure(
        game,
        layout,
        layout.left_figure_x,
        rowline_core::scoreboard::KING_NAME,
        Figure::King,
    ));
    cmds.extend(one_figure(
        game,
        layout,
        layout.right_figure_x,
        rowline_core::scoreboard::CHALLENGER_NAME,
        Figure::Challenger,
    ));
    cmds
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
const PEDESTAL_W: f32 = 26.0;
/// The width of the pedestal's base plate, in logical pixels.
const PEDESTAL_FOOT_W: f32 = 42.0;
/// The height of the pedestal's base plate, in logical pixels.
const PEDESTAL_FOOT_H: f32 = 10.0;
/// The diameter of the figure's medallion, in logical pixels.
const MEDALLION_D: f32 = 44.0;
/// The size of the crown / sword mark inside the medallion.
const MARK: f32 = 26.0;

/// Draw one figure and its pedestal.
///
/// The figure is a medallion -- a disc in the player's colour with the
/// player's mark in the middle -- standing on a metal column, with the
/// player's name under it.
fn one_figure(
    game: &Game,
    layout: Layout,
    slot_x: i32,
    label: &str,
    figure: Figure,
) -> Vec<RenderCmd> {
    let ds = &game.design;
    let p = &ds.palette;
    let mut cmds = Vec::new();

    let slot_x = slot_x as f32;
    let centre = slot_x + rowline_core::layout::FIGURE_W as f32 / 2.0;
    let board_top = layout.board_y as f32;
    let board_bottom = board_top + rowline_core::layout::BOARD_FRAME_H as f32;

    // The medallion sits in the upper third of the board; the column runs
    // from it down to the bottom of the board.
    let top = board_top + (rowline_core::layout::BOARD_FRAME_H as f32 - MEDALLION_D) * 0.36;
    let feet = top + MEDALLION_D;

    // ---- the pedestal ---------------------------------------------------
    let column = rowline_core::render::Rect::new(
        centre - PEDESTAL_W / 2.0,
        feet - 8.0,
        PEDESTAL_W,
        board_bottom - feet + 8.0,
    );
    cmds.push(RenderCmd::Gradient {
        rect: column,
        radius: PEDESTAL_W / 2.0,
        from: p.surface_raised.into(),
        to: p.surface_alt.into(),
        direction: rowline_core::render::GradientDirection::Horizontal,
    });
    cmds.push(RenderCmd::Outline {
        rect: column,
        color: p.outline.into(),
    });
    let foot = rowline_core::render::Rect::new(
        centre - PEDESTAL_FOOT_W / 2.0,
        board_bottom - PEDESTAL_FOOT_H,
        PEDESTAL_FOOT_W,
        PEDESTAL_FOOT_H,
    );
    cmds.push(RenderCmd::RoundedRect {
        rect: foot,
        radius: ds.radius(rowline_theme::Radius::Sm),
        color: p.surface_raised.into(),
    });
    cmds.push(RenderCmd::Outline {
        rect: foot,
        color: p.outline.into(),
    });

    // ---- the medallion -------------------------------------------------
    let colour = match figure {
        Figure::King => p.balls.get(5),
        Figure::Challenger => p.balls.get(4),
    };
    let disc =
        rowline_core::render::Rect::new(centre - MEDALLION_D / 2.0, top, MEDALLION_D, MEDALLION_D);
    cmds.push(RenderCmd::RoundedRect {
        rect: disc,
        radius: MEDALLION_D / 2.0,
        color: colour.into(),
    });
    cmds.push(RenderCmd::Outline {
        rect: disc,
        color: p.outline.into(),
    });

    // ---- the mark: a crown for the king, a sword for the challenger -----
    let icon = match figure {
        Figure::King => crate::commands::ICON_CROWN,
        Figure::Challenger => crate::commands::ICON_SWORD,
    };
    cmds.push(RenderCmd::Icon {
        index: icon,
        dst: rowline_core::input::Point::new(centre - MARK / 2.0, top + (MEDALLION_D - MARK) / 2.0),
        size: rowline_core::input::Size::new(MARK, MARK),
        tint: p.text_on_accent.into(),
    });

    // ---- the label ------------------------------------------------------
    cmds.push(RenderCmd::Text {
        text: label.to_string(),
        at: rowline_core::input::Point::new(centre - 18.0, feet + 6.0),
        size: ds.text_size(rowline_theme::TextRole::Label) as u16,
        color: p.text_secondary.into(),
    });

    cmds
}

/// Draw the leaderboard dialog (总榜).
fn scoreboard_overlay(game: &Game, layout: Layout) -> Vec<RenderCmd> {
    let mut cmds = Vec::new();
    let ds = &game.design;
    let p = &ds.palette;
    let sb = rowline_core::scoreboard::Scoreboard::new(game.scores().to_vec());

    let row_h = 22.0;
    let w = 420.0_f32.min(layout.client_w as f32 - 32.0).max(240.0);
    let h = (rowline_core::scoreboard::ROWS as f32) * row_h + 96.0;
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

    // The sixteen rows, exactly as the original renders them.
    let top = rect.y + 36.0;
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
                color: p.surface_alt.into(),
            });
        }
        cmds.push(RenderCmd::Text {
            text: row.format(),
            at: rowline_core::input::Point::new(rect.x + 16.0, y),
            size: ds.text_size(rowline_theme::TextRole::Body) as u16,
            color: if row.empty {
                p.text_tertiary.into()
            } else {
                p.text_primary.into()
            },
        });
    }

    // The two buttons: Clear and OK.
    let btn_y = rect.y + rect.height - 40.0;
    // The original's buttons are 清除 and 确定(O).
    for (i, (label, colour)) in [
        ("\u{6e05}\u{9664}", p.danger),
        ("\u{786e}\u{5b9a}(O)", p.accent),
    ]
    .iter()
    .enumerate()
    {
        let btn = rowline_core::render::Rect::new(
            rect.x + rect.width * 0.5 + i as f32 * 140.0 - 60.0,
            btn_y,
            120.0,
            30.0,
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
            text: (*label).to_string(),
            at: rowline_core::input::Point::new(btn.x + 46.0, btn.y + 7.0),
            size: ds.text_size(rowline_theme::TextRole::Body) as u16,
            color: (*colour).into(),
        });
    }
    cmds
}

/// Draw the name entry dialog (输入你的名字).
fn name_entry_overlay(game: &Game, layout: Layout) -> Vec<RenderCmd> {
    let mut cmds = Vec::new();
    let ds = &game.design;
    let p = &ds.palette;

    let w = 400.0_f32.min(layout.client_w as f32 - 32.0).max(240.0);
    let h = 150.0;
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

    cmds.push(RenderCmd::Text {
        text: "\u{8f93}\u{5165}\u{4f60}\u{7684}\u{540d}\u{5b57}:".to_string(),
        at: rowline_core::input::Point::new(rect.x + 16.0, rect.y + 46.0),
        size: ds.text_size(rowline_theme::TextRole::Body) as u16,
        color: p.text_primary.into(),
    });

    // The edit field.
    let field =
        rowline_core::render::Rect::new(rect.x + 16.0, rect.y + 72.0, rect.width - 32.0, 30.0);
    cmds.push(RenderCmd::RoundedRect {
        rect: field,
        radius: ds.radius(rowline_theme::Radius::Sm),
        color: p.surface.into(),
    });
    cmds.push(RenderCmd::Outline {
        rect: field,
        color: p.outline_strong.into(),
    });
    cmds.push(RenderCmd::Text {
        text: game.name_input.clone(),
        at: rowline_core::input::Point::new(field.x + 8.0, field.y + 7.0),
        size: ds.text_size(rowline_theme::TextRole::Body) as u16,
        color: p.text_primary.into(),
    });

    // The OK button.
    let btn = rowline_core::render::Rect::new(
        rect.x + (rect.width - 120.0) * 0.5,
        rect.y + rect.height - 44.0,
        120.0,
        30.0,
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
        at: rowline_core::input::Point::new(btn.x + 48.0, btn.y + 7.0),
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
                x + ds.space(rowline_theme::Space::Sm),
                m.bar_height * 0.28,
            ),
            size: ds.text_size(rowline_theme::TextRole::Label) as u16,
            color: if open {
                p.accent.into()
            } else {
                p.text_secondary.into()
            },
        });
    }

    // ---- the toolbar ---------------------------------------------------
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
        // The command's icon, if it has one.
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
        cmds.push(RenderCmd::Text {
            text: item.label.to_string(),
            at: rowline_core::input::Point::new(panel.x + m.pad + 28.0, y + m.row_height * 0.3),
            size: ds.text_size(rowline_theme::TextRole::Body) as u16,
            color: if hovered {
                p.text_primary.into()
            } else {
                p.text_secondary.into()
            },
        });
        if !item.hint.is_empty() {
            cmds.push(RenderCmd::Text {
                text: item.hint.to_string(),
                at: rowline_core::input::Point::new(
                    panel.x + panel.width - m.pad - m.title_width * 0.4,
                    y + m.row_height * 0.3,
                ),
                size: ds.text_size(rowline_theme::TextRole::Caption) as u16,
                color: p.text_tertiary.into(),
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
            .filter(|c| matches!(c, RenderCmd::RoundedRect { .. }))
            .count();
        // 81 cell plates + the HUD bar + the backdrop
        assert!(plates >= 82, "expected 81 cell plates, got {plates}");
        // The backdrop follows the theme, so it is not the pure black of
        // the original any more.
        assert!(cmds.iter().any(|c| matches!(
            c,
            RenderCmd::RoundedRect { color, .. } if *color == rowline_core::render::Color::rgb(27, 30, 38)
        )));
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
    fn the_hud_shows_the_cpu_on_the_left_and_the_player_on_the_right() {
        // docs/11 section 5b: the left box is the CPU's score, the right one
        // the human's.
        let mut g = Game::new(7, RuleModel::default());
        g.engine_mut().set_score(160);
        assert_eq!(g.king_score(), 100);
        assert_eq!(g.challenger_score(), 160);
    }

    #[test]
    fn exit_command_requests_quit() {
        let mut g = Game::new(1, RuleModel::default());
        assert!(!g.quit_requested());
        g.run_command(Command::EXIT);
        assert!(g.quit_requested());
    }
}
