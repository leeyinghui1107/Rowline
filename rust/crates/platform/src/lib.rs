//! Platform abstraction.
//!
//! The game talks to the outside world through four traits:
//!
//! | trait | answers |
//! |---|---|
//! | [`WindowBackend`] | what are the window's size and the pending events? |
//! | [`Clock`] | what time is it? |
//! | [`FileSystem`] | where is the app's data directory, and how do I read/write? |
//! | [`Assets`] | which files exist? (see `rowline-assets`) |
//!
//! Everything above this layer is platform neutral, which is what makes the
//! game testable without a window and portable to every target winit
//! supports.

use std::path::{Path, PathBuf};

use rowline_core::input::{InputEvent, Size};

/// A window and its event queue.
pub trait WindowBackend {
    /// Drain the pending events.
    fn poll_events(&mut self) -> Vec<InputEvent>;
    /// The current drawable size in physical pixels.
    fn size(&self) -> Size;
    /// The display scale factor (2.0 on a typical HiDPI display).
    fn scale_factor(&self) -> f32;
    /// Ask the window to close.
    fn request_quit(&mut self);
    /// Enforce a minimum client size (the layout needs >= 880x892, see
    /// `docs/03` section 3 and `docs/04` decision D3).
    fn set_min_size(&mut self, size: Size);
    /// Set the window title.
    fn set_title(&mut self, title: &str);
}

/// A monotonic millisecond clock.
pub trait Clock {
    /// Milliseconds since an arbitrary but fixed epoch.
    fn now_millis(&self) -> u64;
}

/// The clock used everywhere except in tests.
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now_millis(&self) -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0)
    }
}

/// A file system error.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IoError {
    NotFound(String),
    Denied(String),
    Other(String),
}

impl std::fmt::Display for IoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IoError::NotFound(p) => write!(f, "not found: {p}"),
            IoError::Denied(p) => write!(f, "permission denied: {p}"),
            IoError::Other(m) => write!(f, "I/O error: {m}"),
        }
    }
}

impl std::error::Error for IoError {}

/// Where the game keeps its data.
pub trait FileSystem {
    /// The directory the application owns.
    fn app_dir(&self) -> PathBuf;
    /// Read a whole file.
    fn read(&self, path: &Path) -> Result<Vec<u8>, IoError>;
    /// Write a whole file, creating parent directories as needed.
    fn write(&mut self, path: &Path, bytes: &[u8]) -> Result<(), IoError>;
    /// `true` when the path exists.
    fn exists(&self, path: &Path) -> bool;
}

/// The on-disk implementation, used on every desktop platform.
///
/// The directory follows each platform's convention:
///
/// | platform | location |
/// |---|---|
/// | Linux | `$XDG_DATA_HOME/rowline` or `~/.local/share/rowline` |
/// | macOS | `~/Library/Application Support/rowline` |
/// | Windows | `%APPDATA%\rowline` |
#[cfg(feature = "fs")]
#[derive(Debug, Clone, Default)]
pub struct OsFileSystem {
    dir: PathBuf,
}

#[cfg(feature = "fs")]
impl OsFileSystem {
    /// Resolve the platform's application data directory.
    pub fn app_dir() -> PathBuf {
        if let Ok(p) = std::env::var("XDG_DATA_HOME") {
            if !p.is_empty() {
                return PathBuf::from(p).join("rowline");
            }
        }
        if let Ok(home) = std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE")) {
            let base = PathBuf::from(home);
            if cfg!(target_os = "macos") {
                return base
                    .join("Library")
                    .join("Application Support")
                    .join("rowline");
            }
            if cfg!(target_os = "windows") {
                if let Ok(app) = std::env::var("APPDATA") {
                    if !app.is_empty() {
                        return PathBuf::from(app).join("rowline");
                    }
                }
                return base.join("AppData").join("Roaming").join("rowline");
            }
            return base.join(".local").join("share").join("rowline");
        }
        PathBuf::from(".")
    }

    /// A file system rooted at the platform directory.
    pub fn new() -> Self {
        OsFileSystem {
            dir: Self::app_dir(),
        }
    }
}

#[cfg(feature = "fs")]
impl FileSystem for OsFileSystem {
    fn app_dir(&self) -> PathBuf {
        self.dir.clone()
    }

    fn read(&self, path: &Path) -> Result<Vec<u8>, IoError> {
        std::fs::read(path).map_err(|e| map_io(path, e))
    }

    fn write(&mut self, path: &Path, bytes: &[u8]) -> Result<(), IoError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| map_io(parent, e))?;
        }
        std::fs::write(path, bytes).map_err(|e| map_io(path, e))
    }

    fn exists(&self, path: &Path) -> bool {
        path.exists()
    }
}

#[cfg(feature = "fs")]
fn map_io(path: &Path, e: std::io::Error) -> IoError {
    match e.kind() {
        std::io::ErrorKind::NotFound => IoError::NotFound(path.display().to_string()),
        std::io::ErrorKind::PermissionDenied => IoError::Denied(path.display().to_string()),
        _ => IoError::Other(format!("{}: {e}", path.display())),
    }
}

/// An in-memory file system, used by the head-less backend and by tests.
#[derive(Debug, Default, Clone)]
pub struct MemoryFileSystem {
    dir: PathBuf,
    files: std::collections::BTreeMap<PathBuf, Vec<u8>>,
}

impl MemoryFileSystem {
    /// A fresh, empty file system.
    pub fn new() -> Self {
        MemoryFileSystem {
            dir: PathBuf::from("/app"),
            files: std::collections::BTreeMap::new(),
        }
    }

    /// Preload a file.
    pub fn insert(&mut self, path: &str, bytes: Vec<u8>) {
        self.files.insert(PathBuf::from(path), bytes);
    }
}

impl FileSystem for MemoryFileSystem {
    fn app_dir(&self) -> PathBuf {
        self.dir.clone()
    }

    fn read(&self, path: &Path) -> Result<Vec<u8>, IoError> {
        self.files
            .get(path)
            .cloned()
            .ok_or_else(|| IoError::NotFound(path.display().to_string()))
    }

    fn write(&mut self, path: &Path, bytes: &[u8]) -> Result<(), IoError> {
        self.files.insert(path.to_path_buf(), bytes.to_vec());
        Ok(())
    }

    fn exists(&self, path: &Path) -> bool {
        self.files.contains_key(path)
    }
}

pub mod headless;
#[cfg(feature = "winit")]
pub mod winit_backend;

pub use headless::HeadlessWindow;

/// The product's name, re-exported for the window title.
pub use rowline_core::PRODUCT_NAME;
#[cfg(feature = "winit")]
pub use winit_backend::WinitWindow;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_fs_round_trips() {
        let mut fs = MemoryFileSystem::new();
        let p = Path::new("/app/save.ron");
        assert!(!fs.exists(p));
        fs.write(p, b"hello").unwrap();
        assert!(fs.exists(p));
        assert_eq!(fs.read(p).unwrap(), b"hello");
        assert!(fs.read(Path::new("/app/nope")).is_err());
    }
}
