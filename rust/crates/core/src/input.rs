//! Platform-independent input events.
//!
//! The platform layer translates winit / web / terminal events into these;
//! the game layer never sees a platform type.  That is what makes the game
//! logic testable without a window.

/// A physical key, identified by a stable platform-neutral code.
///
/// The codes are the USB HID usage IDs, so they are identical on every
/// platform and stable across keyboard layouts for the letter/number keys.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum Key {
    KeyA = 0x04,
    KeyB = 0x05,
    KeyC = 0x06,
    KeyD = 0x07,
    KeyE = 0x08,
    KeyF = 0x09,
    KeyG = 0x0A,
    KeyH = 0x0B,
    KeyI = 0x0C,
    KeyJ = 0x0D,
    KeyK = 0x0E,
    KeyL = 0x0F,
    KeyM = 0x10,
    KeyN = 0x11,
    KeyO = 0x12,
    KeyP = 0x13,
    KeyQ = 0x14,
    KeyR = 0x15,
    KeyS = 0x16,
    KeyT = 0x17,
    KeyU = 0x18,
    KeyV = 0x19,
    KeyW = 0x1A,
    KeyX = 0x1B,
    KeyY = 0x1C,
    KeyZ = 0x1D,
    Digit1 = 0x1E,
    Digit2 = 0x1F,
    Digit3 = 0x20,
    Digit4 = 0x21,
    Digit5 = 0x22,
    Digit6 = 0x23,
    Digit7 = 0x24,
    Digit8 = 0x25,
    Digit9 = 0x26,
    Digit0 = 0x27,
    Enter = 0x28,
    Escape = 0x29,
    Backspace = 0x2A,
    Tab = 0x2B,
    Space = 0x2C,
    ArrowLeft = 0x50,
    ArrowRight = 0x51,
    ArrowUp = 0x52,
    ArrowDown = 0x53,
    F1 = 0x3A,
    F2 = 0x3B,
    F3 = 0x3C,
    F4 = 0x3D,
    F5 = 0x3E,
    F6 = 0x3F,
    F7 = 0x40,
    F8 = 0x41,
    F9 = 0x42,
    F10 = 0x43,
    F11 = 0x44,
    F12 = 0x45,
    LeftShift = 0xE1,
    LeftControl = 0xE0,
    LeftAlt = 0xE2,
}

/// A mouse button.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
}

/// Modifier key state at the time of an event.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Modifiers {
    pub shift: bool,
    pub control: bool,
    pub alt: bool,
    pub logo: bool,
}

impl Modifiers {
    /// `true` when no modifier is held.
    pub fn is_empty(&self) -> bool {
        !self.shift && !self.control && !self.alt && !self.logo
    }
}

/// A platform-neutral input event.
#[derive(Clone, Debug, PartialEq)]
pub enum InputEvent {
    /// A key was pressed.
    KeyPressed { key: Key, modifiers: Modifiers },
    /// A key was released.
    KeyReleased { key: Key, modifiers: Modifiers },
    /// A mouse button was pressed.
    MousePressed {
        pos: Point,
        button: MouseButton,
        modifiers: Modifiers,
    },
    /// A mouse button was released.
    MouseReleased {
        pos: Point,
        button: MouseButton,
        modifiers: Modifiers,
    },
    /// The pointer moved.
    MouseMoved { pos: Point },
    /// Committed text from an input method (Chinese/Japanese/Korean IME).
    ///
    /// Physical key events still deliver Latin characters; this carries the
    /// composed text that only the IME can produce.
    TextInput { text: String },
    /// The in-progress composition of an input method.
    ///
    /// An empty string means the composition ended.  While a composition is
    /// active the physical letter keys are producing *pinyin*, not final
    /// text, so the game must not append them to the name.
    ImePreedit { text: String },
    /// The scroll wheel moved (positive = away from the user).
    MouseWheel { delta: f32 },
    /// The drawable area changed size (physical pixels).
    Resized { size: Size },
    /// The display scale factor changed (HiDPI).
    ScaleFactorChanged { scale: f32 },
    /// The user asked to close the application.
    QuitRequested,
}

/// A point in logical pixels.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

impl Point {
    pub const ZERO: Point = Point { x: 0.0, y: 0.0 };

    pub const fn new(x: f32, y: f32) -> Self {
        Point { x, y }
    }
}

/// A size in logical pixels.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Size {
    pub width: f32,
    pub height: f32,
}

impl Size {
    pub const fn new(width: f32, height: f32) -> Self {
        Size { width, height }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modifiers_start_empty() {
        assert!(Modifiers::default().is_empty());
        let m = Modifiers {
            alt: true,
            ..Default::default()
        };
        assert!(!m.is_empty());
    }
}
