//! The winit window backend.
//!
//! Translates winit's events into the platform-neutral [`InputEvent`] set
//! and enforces the minimum client size the layout needs.

use winit::event::{ElementState, MouseButton as WinitMouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::PhysicalKey;
use winit::window::Window;

use rowline_core::input::{InputEvent, Key, Modifiers, MouseButton, Point, Size};

use crate::WindowBackend;

/// A real window.
pub struct WinitWindow {
    window: Window,
    scale: f32,
    size: Size,
    pending: Vec<InputEvent>,
    mods: winit::keyboard::ModifiersState,
    quit: bool,
}

impl WinitWindow {
    /// Create the window inside the winit event loop.
    pub fn new(event_loop: &ActiveEventLoop, title: &str, size: Size, scale: f32) -> Self {
        let attrs = Window::default_attributes()
            .with_title(title)
            .with_inner_size(winit::dpi::PhysicalSize::new(
                (size.width * scale) as u32,
                (size.height * scale) as u32,
            ));
        let window = event_loop
            .create_window(attrs)
            .expect("cannot create window");
        let physical = window.inner_size();
        WinitWindow {
            window,
            scale,
            size: Size::new(physical.width as f32, physical.height as f32),
            pending: Vec::new(),
            mods: Default::default(),
            quit: false,
        }
    }

    /// The window's scale factor (winit reports it as f64).
    pub fn scale_factor_f64(&self) -> f64 {
        self.window.scale_factor()
    }

    /// The underlying winit window (needed to obtain a raw window handle).
    pub fn raw_window(&self) -> &Window {
        &self.window
    }

    /// Convert a winit key event into the neutral representation.
    fn translate_key(key: &PhysicalKey) -> Option<Key> {
        use winit::keyboard::KeyCode;
        let code = match key {
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
            KeyCode::Digit1 => Key::Digit1,
            KeyCode::Digit2 => Key::Digit2,
            KeyCode::Digit3 => Key::Digit3,
            KeyCode::Digit4 => Key::Digit4,
            KeyCode::Digit5 => Key::Digit5,
            KeyCode::Digit6 => Key::Digit6,
            KeyCode::Digit7 => Key::Digit7,
            KeyCode::Digit8 => Key::Digit8,
            KeyCode::Digit9 => Key::Digit9,
            KeyCode::Digit0 => Key::Digit0,
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
}

fn modifiers(state: winit::keyboard::ModifiersState) -> Modifiers {
    Modifiers {
        shift: state.shift_key(),
        control: state.control_key(),
        alt: state.alt_key(),
        logo: state.super_key(),
    }
}

impl WindowBackend for WinitWindow {
    fn poll_events(&mut self) -> Vec<InputEvent> {
        std::mem::take(&mut self.pending)
    }

    fn size(&self) -> Size {
        self.size
    }

    fn scale_factor(&self) -> f32 {
        self.scale
    }

    fn request_quit(&mut self) {
        self.quit = true;
    }

    fn set_min_size(&mut self, size: Size) {
        self.window
            .set_min_inner_size(Some(winit::dpi::LogicalSize::new(size.width, size.height)));
    }

    fn set_title(&mut self, title: &str) {
        self.window.set_title(title);
    }
}

impl WinitWindow {
    /// Feed a winit window event in; returns `true` when the event was
    /// consumed as a neutral input event.
    pub fn handle_window_event(&mut self, event: &WindowEvent) -> bool {
        match event {
            WindowEvent::CloseRequested => {
                self.quit = true;
                true
            }
            WindowEvent::Resized(sz) => {
                self.size = Size::new(sz.width as f32, sz.height as f32);
                self.pending.push(InputEvent::Resized { size: self.size });
                true
            }
            WindowEvent::ModifiersChanged(m) => {
                self.mods = m.state();
                true
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                self.scale = *scale_factor as f32;
                self.pending.push(InputEvent::ScaleFactorChanged {
                    scale: *scale_factor as f32,
                });
                true
            }
            WindowEvent::KeyboardInput { event, .. } => {
                let Some(key) = Self::translate_key(&event.physical_key) else {
                    return false;
                };
                let m = modifiers(self.mods);
                self.pending.push(match event.state {
                    ElementState::Pressed => InputEvent::KeyPressed { key, modifiers: m },
                    ElementState::Released => InputEvent::KeyReleased { key, modifiers: m },
                });
                true
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.pending.push(InputEvent::MouseMoved {
                    pos: Point::new(position.x as f32, position.y as f32),
                });
                true
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let b = match button {
                    WinitMouseButton::Left => MouseButton::Left,
                    WinitMouseButton::Right => MouseButton::Right,
                    WinitMouseButton::Middle => MouseButton::Middle,
                    _ => return false,
                };
                let pos = Point::new(0.0, 0.0);
                self.pending.push(match state {
                    ElementState::Pressed => InputEvent::MousePressed {
                        pos,
                        button: b,
                        modifiers: Modifiers::default(),
                    },
                    ElementState::Released => InputEvent::MouseReleased {
                        pos,
                        button: b,
                        modifiers: Modifiers::default(),
                    },
                });
                true
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let d = match delta {
                    MouseScrollDelta::LineDelta(_, y) => *y,
                    MouseScrollDelta::PixelDelta(p) => p.y as f32 / 40.0,
                };
                self.pending.push(InputEvent::MouseWheel { delta: d });
                true
            }
            _ => false,
        }
    }

    /// `true` once a quit was requested.
    pub fn quit_requested(&self) -> bool {
        self.quit
    }
}
