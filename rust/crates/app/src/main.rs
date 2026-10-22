//! Rowline -- application entry point.
//!
//! ## Display adaptation
//!
//! The shell owns the display policy; the game does not know about it:
//!
//! | concern | where it lives |
//! |---|---|
//! | logical vs physical coordinates | `rowline-display` (`ScalePolicy`, `Viewport`) |
//! | the surface and its format / present mode | `rowline-render` (wgpu backend) |
//! | window events, scale factor, monitors | `rowline-platform` (winit backend) |
//! | the layout in logical units | `rowline-core` (`Layout`) |
//!
//! The loop is:
//!
//! ```text
//! events  ->  game (logical units)  ->  RenderCmd (logical units)
//!         ->  viewport (physical pixels)  ->  wgpu
//! ```

// A release build is a GUI application: on Windows that means the **windows**
// subsystem, so launching it from Explorer does not leave a console window
// behind.  Debug builds keep the console for diagnostics.
#![cfg_attr(
    all(target_os = "windows", not(debug_assertions)),
    windows_subsystem = "windows"
)]

use rowline_assets::AssetLoader;
use rowline_core::input::Size;
use rowline_core::layout::Layout;
use rowline_core::RuleModel;
use rowline_display::{compute_viewport, ScalePolicy};
use rowline_platform::{Clock, SystemClock};

use rowline_app::game::Game;

/// The file system the save lives on.
///
/// With the `fs` feature this is the real disk; without it, an in-memory
/// file system, so the build never touches the user's home directory by
/// accident.
#[cfg(feature = "fs")]
type SaveFs = rowline_platform::OsFileSystem;
#[cfg(not(feature = "fs"))]
type SaveFs = rowline_platform::MemoryFileSystem;

/// Write a diagnostic line without panicking.
///
/// A release build is a GUI-subsystem binary: when it is started from
/// Explorer there is no console, and the `eprintln!` macro panics if
/// writing to stderr fails.  Diagnostics must never take the game down, so
/// the write result is ignored here.
macro_rules! note {
    ($($arg:tt)*) => {{
        use std::io::Write as _;
        let _ = writeln!(std::io::stderr(), $($arg)*);
    }};
}

fn save_fs() -> SaveFs {
    #[cfg(feature = "fs")]
    {
        SaveFs::new()
    }
    #[cfg(not(feature = "fs"))]
    {
        SaveFs::new()
    }
}

/// Load the save, if there is one.
fn load_save(fs: &mut SaveFs) -> Option<rowline_core::save::SaveData> {
    let mut store = rowline_storage::Store::new(fs);
    match store.load() {
        Ok(d) => Some(d),
        Err(e) => {
            note!("rowline: cannot load the save: {e}");
            None
        }
    }
}

/// Write the save.
fn store_save(fs: &mut SaveFs, data: &rowline_core::save::SaveData) {
    let mut store = rowline_storage::Store::new(fs);
    if let Err(e) = store.save(data) {
        note!("rowline: cannot write the save: {e}");
    }
}

fn main() {
    if let Err(e) = run() {
        note!("rowline: {e}");
        std::process::exit(1);
    }
}

// ---------------------------------------------------------------- gpu path
#[cfg(feature = "gpu")]
mod gpu {
    use super::*;
    use rowline_render::Renderer;
    use winit::application::ApplicationHandler;
    use winit::dpi::{LogicalPosition, PhysicalSize, Position};
    use winit::event::{ElementState, Ime, MouseButton as WinitMouseButton, WindowEvent};
    use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
    use winit::keyboard::{KeyCode, PhysicalKey};
    use winit::window::{Fullscreen, Window, WindowId};

    /// The logical client size the window is created at.
    ///
    /// The original's default is a 620x430 **outer** window, which is a
    /// 604x371 client area at 100% Windows metrics; those are the dimensions
    /// its layout formulas and the black-box screenshots were taken at.
    /// Rowline draws its menu inside the client (the original used a native
    /// menu outside it), so the menu strip is added on top, plus the rows
    /// reserved for the window's bottom frame.  The 372-px content is the
    /// smallest height the board fits into, keeping the board origin at the
    /// original (138, 44).
    const INITIAL_LOGICAL: (f64, f64) = (
        604.0,
        372.0
            + rowline_core::layout::MENU_BAR_H as f64
            + rowline_core::layout::FRAME_RESERVE as f64,
    );
    /// The font used for the HUD, in logical units.
    const FONT_SIZE: f32 = 16.0;

    /// The directories that may contain the `assets/` tree.
    ///
    /// The executable's own directory comes first, so a packaged bundle works
    /// when launched from a shortcut or another working directory; the
    /// workspace-relative paths keep `cargo run` working.
    fn asset_roots() -> Vec<std::path::PathBuf> {
        let mut roots = Vec::new();
        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                roots.push(dir.join("assets"));
            }
        }
        roots.push("assets".into());
        roots.push("../assets".into());
        roots.push("../../assets".into());
        roots
    }

    pub struct GpuApp {
        game: Game,
        fs: SaveFs,
        window: Option<&'static Window>,
        renderer: Option<rowline_render::wgpu_backend::WgpuRenderer<'static>>,
        cursor: rowline_core::input::Point,
        /// The display scale factor the window currently sits at.
        scale: f32,
        /// The logical size of the client area.
        logical: Size,
        /// The physical size of the client area.
        physical: Size,
        /// The resolved logical/physical viewport.
        ///
        /// Input positions arrive in physical pixels; dividing by the
        /// viewport maps them back into the content's logical coordinates
        /// (including letterboxing and the retro policy).
        viewport: rowline_display::Viewport,
        mods: rowline_core::input::Modifiers,
        fullscreen: bool,
        /// The last tick, for the effect timing.
        last_tick: u64,
    }

    impl GpuApp {
        fn new() -> Self {
            GpuApp {
                game: Game::new(SystemClock.now_millis() as u32, RuleModel::default()),
                fs: save_fs(),
                window: None,
                renderer: None,
                cursor: rowline_core::input::Point::ZERO,
                scale: 1.0,
                logical: Size::new(INITIAL_LOGICAL.0 as f32, INITIAL_LOGICAL.1 as f32),
                physical: Size::new(0.0, 0.0),
                viewport: compute_viewport(&ScalePolicy::Logical, Size::new(0.0, 0.0), 1.0),
                mods: Default::default(),
                fullscreen: false,
                last_tick: SystemClock.now_millis(),
            }
        }

        /// The game's layout, computed in **logical** units.
        ///
        /// The menu strip is drawn inside the window client, so the play
        /// area is the remaining height; in retro mode the play area is the
        /// fixed 640x480 design space instead of the window size.
        fn layout(&self) -> Layout {
            let (w, h) = if self.game.settings().retro_mode {
                (640, 480)
            } else {
                (self.logical.width as i32, self.logical.height as i32)
            };
            Layout::compute_framed(
                w,
                h,
                rowline_core::layout::MENU_BAR_H,
                rowline_core::layout::FRAME_RESERVE,
            )
        }

        /// Map a physical window position into the content's logical units.
        fn to_game(&self, x: f32, y: f32) -> rowline_core::input::Point {
            let vp = &self.viewport;
            let s = if vp.content_scale > 0.0 {
                vp.content_scale
            } else {
                1.0
            };
            rowline_core::input::Point::new(
                (x - vp.content_rect.x) / s,
                (y - vp.content_rect.y) / s,
            )
        }

        /// Recompute the viewport and hand it to the renderer.
        fn sync_viewport(&mut self) {
            // The retro policy keeps a fixed 640x480 logical resolution and
            // scales it to fit; the default lays the board out for the window.
            let policy = if self.game.settings().retro_mode {
                ScalePolicy::Fixed {
                    width: 640,
                    height: 480,
                    allow_non_integer: false,
                }
            } else {
                ScalePolicy::Logical
            };
            let vp = compute_viewport(&policy, self.physical, self.scale);
            self.viewport = vp;
            let rect = rowline_core::render::Rect::new(
                vp.content_rect.x,
                vp.content_rect.y,
                vp.content_rect.width,
                vp.content_rect.height,
            );
            if let Some(r) = self.renderer.as_mut() {
                r.set_viewport(rect, vp.content_scale);
            }
        }

        /// Load the sprite sheet at the best variant for the current DPR.
        ///
        /// The view builder multiplies its texel coordinates by the sheet's
        /// integer scale, so the loaded scale is recorded on the game.
        fn install_sheet(&mut self) {
            for root in asset_roots() {
                let mut loader = rowline_assets::FileAssetLoader::new(&root);
                let sheet = loader.load_sheet(
                    &rowline_assets::AssetKey::SpriteSheet,
                    rowline_assets::GridSpec::ORIGINAL,
                    self.scale,
                );
                match sheet {
                    Ok(sheet) => {
                        let image = sheet.image();
                        if let Some(r) = self.renderer.as_mut() {
                            r.set_sheet(image.as_raw(), image.width(), image.height());
                        }
                        self.game.set_sheet_scale(sheet.scale());
                        return;
                    }
                    Err(_) => continue,
                }
            }
            note!("rowline: the sprite sheet is missing; run 'make sheet'");
        }

        /// Load the icon atlas.
        fn install_icons(&mut self) {
            // The highest variant that exists wins; the atlas is a strip of
            // square cells and is sampled 1:1 at the device resolution.
            let candidates = ["icons@4x.png", "icons@3x.png", "icons@2x.png", "icons.png"];
            for root in asset_roots() {
                let mut done = false;
                for name in candidates {
                    let p = root.join("icons").join(name);
                    if !p.exists() {
                        continue;
                    }
                    match rowline_assets::load_icon_strip(&p, rowline_app::commands::ICON_COUNT) {
                        Ok(strip) => {
                            if let Some(r) = self.renderer.as_mut() {
                                r.set_icons(&strip.pixels, strip.width, strip.height, strip.count);
                            }
                            done = true;
                            break;
                        }
                        Err(e) => note!("rowline: cannot load the icons: {e}"),
                    }
                }
                if done {
                    break;
                }
            }
        }

        /// Load the font and rasterise it at the current device resolution.
        ///
        /// The bundled font wins; otherwise a system font is used, so the
        /// game always renders text.
        fn install_font(&mut self) {
            let bytes = asset_roots()
                .iter()
                .find_map(|root| std::fs::read(root.join("font.ttf")).ok())
                .or_else(|| rowline_assets::resolve_system_font().map(|(b, _)| b));
            let Some(bytes) = bytes else {
                note!("rowline: no usable font found; text will be omitted");
                return;
            };
            if let Some(r) = self.renderer.as_mut() {
                let _ = r.set_font(bytes, FONT_SIZE, self.scale);
            }
        }

        /// Record the current window geometry in logical units.
        ///
        /// The outer position and the client size are what the original's
        /// INI stores (`WindowX/Y/Width/Height`); persisting them here makes
        /// the next launch reproduce the user's window exactly.
        fn sync_window_geometry(&mut self) {
            let Some(window) = self.window else { return };
            let scale = if self.scale > 0.0 { self.scale } else { 1.0 };
            let size = window.inner_size();
            let (x, y) = window
                .outer_position()
                .map(|p| {
                    (
                        (p.x as f32 / scale).round() as i32,
                        (p.y as f32 / scale).round() as i32,
                    )
                })
                .unwrap_or((self.game.settings().window_x, self.game.settings().window_y));
            self.game.set_window_geometry(
                x,
                y,
                (size.width as f32 / scale).round().max(1.0) as u32,
                (size.height as f32 / scale).round().max(1.0) as u32,
            );
        }

        /// Apply a new physical size: reconfigure the surface and the viewport.
        fn set_physical_size(&mut self, width: u32, height: u32) {
            self.physical = Size::new(width as f32, height as f32);
            self.logical = Size::new(width as f32 / self.scale, height as f32 / self.scale);
            if let Some(r) = self.renderer.as_mut() {
                r.configure_surface(width, height);
            }
            self.sync_viewport();
        }
    }

    impl ApplicationHandler for GpuApp {
        fn resumed(&mut self, event_loop: &ActiveEventLoop) {
            // Restore the settings and the score table before the first frame.
            if let Some(save) = load_save(&mut self.fs) {
                self.game.load_save(save);
            }
            // The scale factor of the monitor the window will appear on.
            let monitor_scale = event_loop
                .primary_monitor()
                .map(|m| m.scale_factor() as f32)
                .unwrap_or(1.0);
            self.scale = monitor_scale;

            // Restore the geometry the original keeps in its INI.  The
            // stored size is the logical client area; the position is the
            // logical outer position.  A negative position means "centre it",
            // which is what the original does when the INI has no entry yet.
            let settings = self.game.settings().clone();
            let min = rowline_app::game::minimum_client_size();
            let min_w = min.width.max(1.0);
            let min_h = min.height
                + rowline_core::layout::MENU_BAR_H as f32
                + rowline_core::layout::FRAME_RESERVE as f32;
            let want_w = (settings.window_width as f32).max(min_w);
            let want_h = (settings.window_height as f32).max(min_h);
            let mut attrs = Window::default_attributes()
                .with_title(rowline_core::PRODUCT_NAME)
                // A *logical* size: the window looks the same on every
                // monitor, whatever its scale factor is.
                .with_inner_size(PhysicalSize::new(
                    (want_w * monitor_scale) as u32,
                    (want_h * monitor_scale) as u32,
                ))
                // The layout needs at least this much room (docs/03 S3).
                .with_min_inner_size(PhysicalSize::new(
                    (min_w * monitor_scale) as u32,
                    (min_h * monitor_scale) as u32,
                ));
            let position = if settings.window_x >= 0 && settings.window_y >= 0 {
                Some(Position::Logical(LogicalPosition::new(
                    settings.window_x as f64,
                    settings.window_y as f64,
                )))
            } else {
                event_loop.primary_monitor().map(|m| {
                    let size = m.size();
                    let mw = size.width as f64 / monitor_scale as f64;
                    let mh = size.height as f64 / monitor_scale as f64;
                    Position::Logical(LogicalPosition::new(
                        ((mw - want_w as f64) * 0.5).max(0.0),
                        ((mh - want_h as f64) * 0.5).max(0.0),
                    ))
                })
            };
            if let Some(p) = position {
                attrs = attrs.with_position(p);
            }
            let window = match event_loop.create_window(attrs) {
                Ok(w) => w,
                Err(e) => {
                    note!("rowline: cannot create window: {e}");
                    event_loop.exit();
                    return;
                }
            };
            let size = window.inner_size();
            self.scale = window.scale_factor() as f32;
            self.set_physical_size(size.width, size.height);

            // The window lives for the whole program; leaking it gives the
            // wgpu surface a 'static lifetime and removes a self-referential
            // borrow between the window and the renderer.
            let window: &'static Window = Box::leak(Box::new(window));
            // The name-entry dialog accepts composed text (Chinese IME), just
            // like the original's edit control.  The cursor area is placed
            // near the field so the IME candidate window does not cover it.
            window.set_ime_allowed(true);
            window.set_ime_cursor_area(
                winit::dpi::LogicalPosition::new(120.0, 150.0),
                winit::dpi::LogicalSize::new(120.0, 24.0),
            );

            let instance =
                wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
            let surface = match instance.create_surface(window) {
                Ok(s) => s,
                Err(e) => {
                    note!("rowline: cannot create a GPU surface: {e}");
                    event_loop.exit();
                    return;
                }
            };
            match rowline_render::wgpu_backend::WgpuRenderer::new(
                &instance,
                surface,
                size.width.max(1),
                size.height.max(1),
            ) {
                Ok(r) => self.renderer = Some(r),
                Err(e) => {
                    note!("rowline: cannot create the renderer: {e}");
                    event_loop.exit();
                }
            }
            self.window = Some(window);
            self.install_font();
            self.install_icons();
            self.install_sheet();
            self.sync_viewport();
        }

        fn window_event(
            &mut self,
            event_loop: &ActiveEventLoop,
            _window_id: WindowId,
            event: WindowEvent,
        ) {
            let layout = self.layout();
            match event {
                WindowEvent::CloseRequested => {
                    self.sync_window_geometry();
                    event_loop.exit();
                }

                // ---- the window changed size ---------------------------------
                WindowEvent::Resized(size) => {
                    self.set_physical_size(size.width, size.height);
                    self.sync_window_geometry();
                    self.game.handle_event(
                        &rowline_core::input::InputEvent::Resized {
                            size: self.physical,
                        },
                        layout,
                    );
                }

                WindowEvent::Moved(_) => self.sync_window_geometry(),

                // ---- the window moved to a monitor with another DPI --------
                //
                // winit asks us to acknowledge the new scale factor by
                // writing the *logical* inner size back.  Doing so keeps the
                // window the same apparent size across the transition,
                // which is what a per-monitor-DPI-aware application does.
                WindowEvent::ScaleFactorChanged {
                    scale_factor,
                    mut inner_size_writer,
                } => {
                    let old_logical_w = self.logical.width;
                    let old_logical_h = self.logical.height;
                    self.scale = scale_factor as f32;
                    // winit 0.30 asks for a *physical* size: multiply the
                    // logical size we want to keep by the new scale factor.
                    if let Err(e) = inner_size_writer.request_inner_size(PhysicalSize::new(
                        (old_logical_w * scale_factor as f32) as u32,
                        (old_logical_h * scale_factor as f32) as u32,
                    )) {
                        note!("rowline: cannot keep the logical size: {e}");
                    }
                    // The physical size follows from the new scale factor.
                    let physical = self.window.map(|w| w.inner_size());
                    if let Some(s) = physical {
                        self.set_physical_size(s.width, s.height);
                    }
                    // The glyphs must be rasterised again at the new
                    // resolution, otherwise the type looks blurry.
                    if let Some(r) = self.renderer.as_mut() {
                        r.set_font_dpr(self.scale);
                    }
                    // A new DPR may need a higher-resolution sheet variant.
                    self.install_sheet();
                    self.game.handle_event(
                        &rowline_core::input::InputEvent::ScaleFactorChanged {
                            scale: scale_factor as f32,
                        },
                        layout,
                    );
                }

                WindowEvent::ModifiersChanged(m) => {
                    self.mods = rowline_core::input::Modifiers {
                        shift: m.state().shift_key(),
                        control: m.state().control_key(),
                        alt: m.state().alt_key(),
                        logo: m.state().super_key(),
                    };
                }
                WindowEvent::CursorMoved { position, .. } => {
                    // winit reports physical pixels; the game thinks in the
                    // content's logical units.  Without this conversion every
                    // click misses on a HiDPI display.
                    self.cursor = self.to_game(position.x as f32, position.y as f32);
                    self.game.handle_event(
                        &rowline_core::input::InputEvent::MouseMoved { pos: self.cursor },
                        layout,
                    );
                }
                WindowEvent::MouseInput { state, button, .. } => {
                    let b = match button {
                        WinitMouseButton::Left => rowline_core::input::MouseButton::Left,
                        WinitMouseButton::Right => rowline_core::input::MouseButton::Right,
                        WinitMouseButton::Middle => rowline_core::input::MouseButton::Middle,
                        _ => return,
                    };
                    let ev = match state {
                        ElementState::Pressed => rowline_core::input::InputEvent::MousePressed {
                            pos: self.cursor,
                            button: b,
                            modifiers: self.mods,
                        },
                        ElementState::Released => rowline_core::input::InputEvent::MouseReleased {
                            pos: self.cursor,
                            button: b,
                            modifiers: self.mods,
                        },
                    };
                    self.game.handle_event(&ev, layout);
                }
                WindowEvent::Ime(Ime::Commit(text)) => {
                    self.game
                        .handle_event(&rowline_core::input::InputEvent::TextInput { text }, layout);
                }
                WindowEvent::Ime(Ime::Preedit(text, _)) => {
                    // Track the composition so the raw pinyin typed by the
                    // physical keys is replaced by the committed characters.
                    self.game.handle_event(
                        &rowline_core::input::InputEvent::ImePreedit { text },
                        layout,
                    );
                }
                WindowEvent::KeyboardInput { event, .. } => {
                    let Some(key) = translate_key(&event) else {
                        return;
                    };
                    let ev = match event.state {
                        ElementState::Pressed => rowline_core::input::InputEvent::KeyPressed {
                            key,
                            modifiers: self.mods,
                        },
                        ElementState::Released => rowline_core::input::InputEvent::KeyReleased {
                            key,
                            modifiers: self.mods,
                        },
                    };
                    self.game.handle_event(&ev, layout);
                    // Ctrl+T / Ctrl+M / Ctrl+- toggle the presentation
                    // preferences; they never touch the game state.
                    // The platform's command modifier: Cmd on macOS, Ctrl
                    // elsewhere.  The original is Windows-only, so this is
                    // the cross-platform equivalent of its Ctrl shortcuts.
                    let command_mod = if cfg!(target_os = "macos") {
                        self.mods.logo
                    } else {
                        self.mods.control
                    };
                    if matches!(event.state, ElementState::Pressed) && command_mod {
                        match key {
                            rowline_core::input::Key::KeyT => self.game.toggle_theme(),
                            rowline_core::input::Key::KeyM => self.game.toggle_reduced_motion(),
                            rowline_core::input::Key::KeyE => self.game.cycle_text_scale(),
                            rowline_core::input::Key::KeyR => {
                                self.game.toggle_retro_mode();
                            }
                            _ => {}
                        }
                    }
                    // F11 toggles full screen.
                    if matches!(event.state, ElementState::Pressed)
                        && matches!(key, rowline_core::input::Key::F11)
                    {
                        self.toggle_fullscreen();
                    }
                }
                _ => {}
            }
        }

        fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
            if self.game.quit_requested() {
                event_loop.exit();
                return;
            }
            // Advance the decorative effects.
            let now = SystemClock.now_millis();
            let dt = ((now - self.last_tick) as f32 / 1000.0).min(0.1);
            self.last_tick = now;
            let layout = self.layout();
            self.game.tick(dt, layout);
            // Keep the loop running while an animation is in flight.  In the
            // default Wait mode a click is the last event, so the move would
            // otherwise advance exactly one frame and freeze.
            if self.game.is_animating() {
                event_loop.set_control_flow(ControlFlow::Poll);
                if let Some(window) = self.window {
                    window.request_redraw();
                }
            } else {
                event_loop.set_control_flow(ControlFlow::Wait);
            }
            // A menu command (固定分辨率) may have changed the display policy.
            if self.game.take_viewport_dirty() {
                self.sync_viewport();
            }
            let layout = self.layout();
            if let Some(r) = self.renderer.as_mut() {
                r.begin(self.physical, self.scale);
                for cmd in rowline_app::game::frame(&self.game, layout) {
                    r.submit(cmd);
                }
                r.end();
            }
        }
    }

    impl GpuApp {
        /// Toggle between windowed and borderless full screen.
        fn toggle_fullscreen(&mut self) {
            let Some(window) = self.window else { return };
            // winit 0.30 has no `Fullscreen::None`; leaving full screen is
            // `set_fullscreen(None)`.
            let next = if self.fullscreen {
                None
            } else {
                Some(Fullscreen::Borderless(None))
            };
            window.set_fullscreen(next);
            self.fullscreen = !self.fullscreen;
        }
    }

    fn translate_key(event: &winit::event::KeyEvent) -> Option<rowline_core::input::Key> {
        use rowline_core::input::Key;
        let code = match event.physical_key {
            PhysicalKey::Code(c) => c,
            _ => return None,
        };
        Some(match code {
            KeyCode::KeyA => Key::KeyA,
            KeyCode::KeyB => Key::KeyB,
            KeyCode::KeyC => Key::KeyC,
            KeyCode::KeyD => Key::KeyD,
            KeyCode::KeyE => Key::KeyE,
            KeyCode::KeyF => Key::KeyF,
            KeyCode::KeyG => Key::KeyG,
            KeyCode::KeyH => Key::KeyH,
            KeyCode::KeyI => Key::KeyI,
            KeyCode::KeyJ => Key::KeyJ,
            KeyCode::KeyK => Key::KeyK,
            KeyCode::KeyL => Key::KeyL,
            KeyCode::KeyM => Key::KeyM,
            KeyCode::KeyN => Key::KeyN,
            KeyCode::KeyO => Key::KeyO,
            KeyCode::KeyP => Key::KeyP,
            KeyCode::KeyQ => Key::KeyQ,
            KeyCode::KeyR => Key::KeyR,
            KeyCode::KeyS => Key::KeyS,
            KeyCode::KeyT => Key::KeyT,
            KeyCode::KeyU => Key::KeyU,
            KeyCode::KeyV => Key::KeyV,
            KeyCode::KeyW => Key::KeyW,
            KeyCode::KeyX => Key::KeyX,
            KeyCode::KeyY => Key::KeyY,
            KeyCode::KeyZ => Key::KeyZ,
            KeyCode::Digit0 => Key::Digit0,
            KeyCode::Digit1 => Key::Digit1,
            KeyCode::Digit2 => Key::Digit2,
            KeyCode::Digit3 => Key::Digit3,
            KeyCode::Digit4 => Key::Digit4,
            KeyCode::Digit5 => Key::Digit5,
            KeyCode::Digit6 => Key::Digit6,
            KeyCode::Digit7 => Key::Digit7,
            KeyCode::Digit8 => Key::Digit8,
            KeyCode::Digit9 => Key::Digit9,
            KeyCode::Enter => Key::Enter,
            KeyCode::Escape => Key::Escape,
            KeyCode::Backspace => Key::Backspace,
            KeyCode::Tab => Key::Tab,
            KeyCode::Space => Key::Space,
            KeyCode::ArrowLeft => Key::ArrowLeft,
            KeyCode::ArrowRight => Key::ArrowRight,
            KeyCode::ArrowUp => Key::ArrowUp,
            KeyCode::ArrowDown => Key::ArrowDown,
            KeyCode::F1 => Key::F1,
            KeyCode::F2 => Key::F2,
            KeyCode::F3 => Key::F3,
            KeyCode::F4 => Key::F4,
            KeyCode::F5 => Key::F5,
            KeyCode::F6 => Key::F6,
            KeyCode::F7 => Key::F7,
            KeyCode::F8 => Key::F8,
            KeyCode::F9 => Key::F9,
            KeyCode::F10 => Key::F10,
            KeyCode::F11 => Key::F11,
            KeyCode::F12 => Key::F12,
            KeyCode::ShiftLeft => Key::LeftShift,
            KeyCode::ControlLeft => Key::LeftControl,
            KeyCode::AltLeft => Key::LeftAlt,
            _ => return None,
        })
    }

    pub fn run() -> Result<(), Box<dyn std::error::Error>> {
        let event_loop = EventLoop::new()?;
        let mut app = GpuApp::new();
        event_loop
            .run_app(&mut app)
            .map_err(|e| format!("event loop: {e}"))?;
        // Persist the settings and the scores on the way out.
        app.game.sync_settings();
        store_save(&mut app.fs, &app.game.to_save());
        Ok(())
    }
}

#[cfg(feature = "gpu")]
fn run() -> Result<(), Box<dyn std::error::Error>> {
    gpu::run()
}

// ---------------------------------------------------------------- headless path
#[cfg(not(feature = "gpu"))]
fn run() -> Result<(), Box<dyn std::error::Error>> {
    use rowline_platform::HeadlessWindow;
    use rowline_platform::WindowBackend;
    use rowline_render::Renderer;

    let mut window = HeadlessWindow::default();
    let min = rowline_app::game::minimum_client_size();
    window.set_min_size(Size::new(
        min.width,
        min.height
            + rowline_core::layout::MENU_BAR_H as f32
            + rowline_core::layout::FRAME_RESERVE as f32,
    ));
    let clock = SystemClock;
    let mut game = Game::new(clock.now_millis() as u32, RuleModel::default());
    let mut fs = save_fs();
    if let Some(save) = load_save(&mut fs) {
        game.load_save(save);
    }
    let mut renderer = rowline_render::RecordingRenderer::new();
    let mut last = clock.now_millis();

    loop {
        let scale = window.scale_factor();
        let physical = window.size();
        let logical = Size::new(physical.width / scale, physical.height / scale);
        let layout = Layout::compute_framed(
            logical.width as i32,
            logical.height as i32,
            rowline_core::layout::MENU_BAR_H,
            rowline_core::layout::FRAME_RESERVE,
        );
        let vp = compute_viewport(&ScalePolicy::Logical, physical, scale);
        renderer.set_viewport(
            rowline_core::render::Rect::new(
                vp.content_rect.x,
                vp.content_rect.y,
                vp.content_rect.width,
                vp.content_rect.height,
            ),
            vp.content_scale,
        );

        for ev in window.poll_events() {
            game.handle_event(&ev, layout);
        }
        if game.quit_requested() {
            break;
        }

        renderer.begin(physical, scale);
        for cmd in rowline_app::game::frame(&game, layout) {
            renderer.submit(cmd);
        }
        renderer.end();

        let now = clock.now_millis();
        if now - last < 8 {
            std::thread::sleep(std::time::Duration::from_millis(8 - (now - last)));
        }
        last = now;
    }
    game.sync_settings();
    store_save(&mut fs, &game.to_save());
    Ok(())
}
