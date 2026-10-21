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
            eprintln!("rowline: cannot load the save: {e}");
            None
        }
    }
}

/// Write the save.
fn store_save(fs: &mut SaveFs, data: &rowline_core::save::SaveData) {
    let mut store = rowline_storage::Store::new(fs);
    if let Err(e) = store.save(data) {
        eprintln!("rowline: cannot write the save: {e}");
    }
}

fn main() {
    if let Err(e) = run() {
        eprintln!("rowline: {e}");
        std::process::exit(1);
    }
}

// ---------------------------------------------------------------- gpu path
#[cfg(feature = "gpu")]
mod gpu {
    use super::*;
    use rowline_render::Renderer;
    use winit::application::ApplicationHandler;
    use winit::dpi::PhysicalSize;
    use winit::event::{ElementState, MouseButton as WinitMouseButton, WindowEvent};
    use winit::event_loop::{ActiveEventLoop, EventLoop};
    use winit::keyboard::{KeyCode, PhysicalKey};
    use winit::window::{Fullscreen, Window, WindowId};

    /// The logical size the window is created at.
    const INITIAL_LOGICAL: (f64, f64) = (1280.0, 960.0);
    /// The font used for the HUD, in logical units.
    const FONT_SIZE: f32 = 16.0;
    /// Where the font lives, relative to the working directory.
    const FONT_PATH: &str = "assets/font.ttf";

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
                mods: Default::default(),
                fullscreen: false,
                last_tick: SystemClock.now_millis(),
            }
        }

        /// The game's layout, computed in **logical** units.
        fn layout(&self) -> Layout {
            Layout::compute(self.logical.width as i32, self.logical.height as i32)
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

        /// Load the icon atlas.
        fn install_icons(&mut self) {
            // The highest variant that exists wins; the atlas is a strip of
            // square cells and is sampled 1:1 at the device resolution.
            let candidates = [
                "assets/icons/icons@3x.png",
                "assets/icons/icons@2x.png",
                "assets/icons/icons.png",
            ];
            for path in candidates {
                let p = std::path::Path::new(path);
                if !p.exists() {
                    continue;
                }
                match rowline_assets::load_icon_strip(p, rowline_app::commands::ICON_COUNT) {
                    Ok(strip) => {
                        if let Some(r) = self.renderer.as_mut() {
                            r.set_icons(&strip.pixels, strip.width, strip.height, strip.count);
                        }
                        break;
                    }
                    Err(e) => eprintln!("rowline: cannot load the icons: {e}"),
                }
            }
        }

        /// Load the font and rasterise it at the current device resolution.
        ///
        /// The bundled font wins; otherwise a system font is used, so the
        /// game always renders text.
        fn install_font(&mut self) {
            let bytes = std::fs::read(FONT_PATH)
                .ok()
                .or_else(|| rowline_assets::resolve_system_font().map(|(b, _)| b));
            let Some(bytes) = bytes else {
                eprintln!("rowline: no usable font found; text will be omitted");
                return;
            };
            if let Some(r) = self.renderer.as_mut() {
                let _ = r.set_font(bytes, FONT_SIZE, self.scale);
            }
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

            let attrs = Window::default_attributes()
                .with_title(rowline_core::PRODUCT_NAME)
                // A *logical* size: the window looks the same on every
                // monitor, whatever its scale factor is.
                .with_inner_size(PhysicalSize::new(
                    (INITIAL_LOGICAL.0 * monitor_scale as f64) as u32,
                    (INITIAL_LOGICAL.1 * monitor_scale as f64) as u32,
                ))
                // The layout needs at least this much room (docs/03 S3).
                .with_min_inner_size(PhysicalSize::new(
                    (rowline_app::game::minimum_client_size().width * monitor_scale) as u32,
                    (rowline_app::game::minimum_client_size().height * monitor_scale) as u32,
                ));
            let window = match event_loop.create_window(attrs) {
                Ok(w) => w,
                Err(e) => {
                    eprintln!("rowline: cannot create window: {e}");
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

            let instance =
                wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
            let surface = match instance.create_surface(window) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("rowline: cannot create a GPU surface: {e}");
                    event_loop.exit();
                    return;
                }
            };
            match rowline_render::wgpu_backend::WgpuRenderer::new(
                surface,
                size.width.max(1),
                size.height.max(1),
            ) {
                Ok(r) => self.renderer = Some(r),
                Err(e) => {
                    eprintln!("rowline: cannot create the renderer: {e}");
                    event_loop.exit();
                }
            }
            self.window = Some(window);
            self.install_font();
            self.install_icons();
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
                WindowEvent::CloseRequested => event_loop.exit(),

                // ---- the window changed size ---------------------------------
                WindowEvent::Resized(size) => {
                    self.set_physical_size(size.width, size.height);
                    self.game.handle_event(
                        &rowline_core::input::InputEvent::Resized {
                            size: self.physical,
                        },
                        layout,
                    );
                }

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
                        eprintln!("rowline: cannot keep the logical size: {e}");
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
                    self.cursor =
                        rowline_core::input::Point::new(position.x as f32, position.y as f32);
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
                    if matches!(event.state, ElementState::Pressed) && self.mods.control {
                        match key {
                            rowline_core::input::Key::KeyT => self.game.toggle_theme(),
                            rowline_core::input::Key::KeyM => self.game.toggle_reduced_motion(),
                            rowline_core::input::Key::KeyE => self.game.cycle_text_scale(),
                            rowline_core::input::Key::KeyR => {
                                self.game.toggle_retro_mode();
                                self.sync_viewport();
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
            self.game.tick(dt);
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
    window.set_min_size(rowline_app::game::minimum_client_size());
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
        let layout = Layout::compute(logical.width as i32, logical.height as i32);
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
