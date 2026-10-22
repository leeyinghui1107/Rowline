//! Asset loading.
//!
//! Two kinds of asset exist in this game:
//!
//! * the **sprite sheet** -- a single 576x586 image whose grid is the
//!   contract described in `docs/05_ART_SPEC.md`;
//! * **data files** -- settings and the high-score table, encoded with a
//!   pluggable [`rowline_core::SaveFormat`].
//!
//! The sheet exists at several integer scales (`@1x`, `@2x`, `@3x`).  The
//! loader picks the smallest variant that still covers the physical pixels
//! the asset will occupy, so the GPU samples it 1:1 and it stays sharp on a
//! high-DPI display.  When no variant is large enough, the largest one is
//! **resampled at runtime with a Lanczos-3 kernel** (`rowline-display`),
//! which is far better than a bilinear stretch.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use image::RgbaImage;
use rowline_core::render::Rect;

/// The grid a sprite sheet is divided into, at scale 1.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GridSpec {
    /// Width of one slot, in texels (at scale 1).
    pub cell: u32,
    /// Number of slots across.
    pub cols: u32,
    /// Number of slots down.
    pub rows: u32,
    /// Row of texels the sampled band starts at (at scale 1).
    pub origin_y: u32,
}

impl GridSpec {
    /// The grid used by the original atlas (see `docs/05` section 1).
    pub const ORIGINAL: GridSpec = GridSpec {
        cell: 36,
        cols: 16,
        rows: 10,
        origin_y: 225,
    };

    /// The same grid at an integer scale.
    pub fn scaled(&self, scale: u32) -> GridSpec {
        let s = scale.max(1);
        GridSpec {
            cell: self.cell * s,
            cols: self.cols,
            rows: self.rows,
            origin_y: self.origin_y * s,
        }
    }
}

/// A loaded sprite sheet.
#[derive(Clone, Debug)]
pub struct SpriteSheet {
    image: RgbaImage,
    grid: GridSpec,
    /// The integer scale this sheet was authored at (1, 2, 3, ...).
    scale: u32,
}

impl SpriteSheet {
    /// Wrap an image with a grid description.
    ///
    /// `grid` is the **scale-1** description; it is scaled here so that the
    /// caller cannot forget to do it.
    pub fn new(image: RgbaImage, grid: GridSpec, scale: u32) -> Self {
        let scale = scale.max(1);
        SpriteSheet {
            image,
            grid: grid.scaled(scale),
            scale,
        }
    }

    /// The grid description (already scaled).
    pub fn grid(&self) -> GridSpec {
        self.grid
    }

    /// The integer scale of the sheet.
    pub fn scale(&self) -> u32 {
        self.scale
    }

    /// The underlying image.
    pub fn image(&self) -> &RgbaImage {
        &self.image
    }

    /// The source rectangle of a slot by column index.
    pub fn slot(&self, col: u32) -> Rect {
        let cell = self.grid.cell as f32;
        Rect::new(
            (col * self.grid.cell) as f32,
            self.grid.origin_y as f32,
            cell,
            cell,
        )
    }

    /// The source rectangle of a ball.
    ///
    /// This is the **index contract** recovered from the original:
    ///
    /// ```text
    /// src_x = value * 36 * scale + sprite_set * 288 * scale
    /// src_y = 225 * scale
    /// ```
    pub fn ball(&self, value: u8, sprite_set: u8) -> Rect {
        let col = value as u32 + sprite_set as u32 * 8;
        self.slot(col.min(self.grid.cols.saturating_sub(1)))
    }
}

/// An asset identifier.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AssetKey {
    /// The main sprite sheet.
    SpriteSheet,
    /// A named file relative to the asset root.
    File(String),
}

/// Something that went wrong while loading.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AssetError {
    /// The asset does not exist.
    NotFound(String),
    /// The file could not be read.
    Io(String),
    /// The file could not be decoded.
    Decode(String),
}

impl std::fmt::Display for AssetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AssetError::NotFound(k) => write!(f, "asset not found: {k}"),
            AssetError::Io(m) => write!(f, "asset I/O failed: {m}"),
            AssetError::Decode(m) => write!(f, "asset decode failed: {m}"),
        }
    }
}

impl std::error::Error for AssetError {}

/// Loads assets from somewhere.
pub trait AssetLoader {
    /// Load a sprite sheet by key, at the best scale for `scale_factor`.
    fn load_sheet(
        &mut self,
        key: &AssetKey,
        grid: GridSpec,
        scale_factor: f32,
    ) -> Result<SpriteSheet, AssetError>;

    /// Load an opaque blob (audio, fonts, data files).
    fn load_bytes(&mut self, key: &AssetKey) -> Result<Vec<u8>, AssetError>;

    /// The integer scales the loader can offer for a key.
    fn available_scales(&self, key: &AssetKey) -> Vec<u32>;
}

/// The file name of a sheet variant, e.g. `rowline_sheet@2x.png`.
pub fn variant_name(base: &str, scale: u32) -> String {
    if scale <= 1 {
        format!("{base}.png")
    } else {
        format!("{base}@{scale}x.png")
    }
}

// ---------------------------------------------------------------- embedded
//
// The product ships as a **single executable**: the sprite sheet and the
// icon strip are compiled into it, so there is no `assets/` directory to
// lose, copy or tamper with.  The PNGs are already compressed; embedding
// them costs about 190 KB of binary size.

/// The sprite-sheet variants compiled into the binary.
pub const EMBEDDED_SHEET_SCALES: [u32; 4] = [1, 2, 3, 4];

/// The icon-strip variants compiled into the binary.
pub const EMBEDDED_ICON_SCALES: [u32; 4] = [1, 2, 3, 4];

/// The embedded sprite sheet at `scale`, if that variant exists.
pub fn embedded_sheet(scale: u32) -> Option<&'static [u8]> {
    match scale {
        1 => Some(include_bytes!("../../../../assets/sheet/rowline_sheet.png")),
        2 => Some(include_bytes!(
            "../../../../assets/sheet/rowline_sheet@2x.png"
        )),
        3 => Some(include_bytes!(
            "../../../../assets/sheet/rowline_sheet@3x.png"
        )),
        4 => Some(include_bytes!(
            "../../../../assets/sheet/rowline_sheet@4x.png"
        )),
        _ => None,
    }
}

/// The embedded icon strip at `scale`, if that variant exists.
pub fn embedded_icons(scale: u32) -> Option<&'static [u8]> {
    match scale {
        1 => Some(include_bytes!("../../../../assets/icons/icons.png")),
        2 => Some(include_bytes!("../../../../assets/icons/icons@2x.png")),
        3 => Some(include_bytes!("../../../../assets/icons/icons@3x.png")),
        4 => Some(include_bytes!("../../../../assets/icons/icons@4x.png")),
        _ => None,
    }
}

/// Decode an embedded sprite sheet.
pub fn embedded_sheet_image(scale: u32) -> Result<SpriteSheet, AssetError> {
    let bytes = embedded_sheet(scale)
        .ok_or_else(|| AssetError::NotFound(format!("embedded sheet @{scale}x")))?;
    let image = decode_png(bytes)?;
    Ok(SpriteSheet::new(image, GridSpec::ORIGINAL, scale))
}

/// Decode an icon strip held in memory.
pub fn load_icon_strip_bytes(bytes: &[u8], count: u32) -> Result<IconStrip, AssetError> {
    let image = decode_png(bytes)?;
    Ok(IconStrip {
        width: image.width(),
        height: image.height(),
        count: if count == 0 { 0 } else { count },
        pixels: image.into_raw(),
    })
}

/// Decode a PNG into an RGBA8 image.
pub fn decode_png(bytes: &[u8]) -> Result<RgbaImage, AssetError> {
    image::load_from_memory_with_format(bytes, image::ImageFormat::Png)
        .map(|i| i.to_rgba8())
        .map_err(|e| AssetError::Decode(e.to_string()))
}

/// Resample an image to an integer multiple of its size with Lanczos-3.
pub fn upscale_lanczos(img: &RgbaImage, scale: u32) -> RgbaImage {
    let s = scale.max(1);
    if s == 1 {
        return img.clone();
    }
    let (w, h) = (img.width(), img.height());
    let raw = img.as_raw();
    let out = rowline_display::lanczos3_rgba(raw, w, h, w * s, h * s);
    RgbaImage::from_raw(w * s, h * s, out).unwrap_or_else(|| img.clone())
}

/// An icon atlas: a horizontal strip of square cells.
#[derive(Clone, Debug)]
pub struct IconStrip {
    /// The RGBA8 pixels of the whole strip.
    pub pixels: Vec<u8>,
    pub width: u32,
    pub height: u32,
    /// The number of icons.
    pub count: u32,
}

impl IconStrip {
    /// The source rectangle of an icon, in texels.
    pub fn source(&self, index: u32) -> (u32, u32, u32, u32) {
        let cell = self.height;
        (index * cell, 0, cell, cell)
    }
}

/// Load an icon strip from a PNG file.
pub fn load_icon_strip(path: &Path, count: u32) -> Result<IconStrip, AssetError> {
    let bytes =
        std::fs::read(path).map_err(|e| AssetError::Io(format!("{}: {e}", path.display())))?;
    load_icon_strip_bytes(&bytes, count)
}

/// Resolve a usable system font.
///
/// The game ships no font file: a TrueType face is looked up in the usual
/// per-platform locations, and the first one that loads wins.  This keeps
/// the repository free of a binary asset while still rendering text.
pub fn resolve_system_font() -> Option<(Vec<u8>, &'static str)> {
    const CANDIDATES: &[&str] = &[
        // The interface is Simplified Chinese, so a face with CJK coverage
        // is preferred; the Latin-only faces are the fallback.
        //
        // macOS
        "/System/Library/Fonts/PingFang.ttc",
        "/System/Library/Fonts/Hiragino Sans GB.ttc",
        "/System/Library/Fonts/Supplemental/Songti.ttc",
        "/Library/Fonts/Arial Unicode.ttf",
        "/System/Library/Fonts/Supplemental/Arial.ttf",
        "/System/Library/Fonts/Supplemental/Verdana.ttf",
        "/System/Library/Fonts/Helvetica.ttc",
        // Linux
        "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/truetype/noto/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/truetype/wqy/wqy-zenhei.ttc",
        "/usr/share/fonts/truetype/arphic/uming.ttc",
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        "/usr/share/fonts/TTF/DejaVuSans.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf",
        "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf",
        "/usr/share/fonts/truetype/noto/NotoSans-Regular.ttf",
        // Windows
        "C:\\Windows\\Fonts\\msyh.ttc",
        "C:\\Windows\\Fonts\\simsun.ttc",
        "C:\\Windows\\Fonts\\arial.ttf",
        "C:\\Windows\\Fonts\\segoeui.ttf",
        "C:\\Windows\\Fonts\\tahoma.ttf",
    ];
    for path in CANDIDATES {
        if let Ok(bytes) = std::fs::read(path) {
            return Some((bytes, path));
        }
    }
    None
}

/// Loads assets from a directory on disk.
#[derive(Debug, Clone)]
pub struct FileAssetLoader {
    root: PathBuf,
}

impl FileAssetLoader {
    /// A loader rooted at `root`.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        FileAssetLoader { root: root.into() }
    }

    /// The root directory.
    pub fn root(&self) -> &Path {
        &self.root
    }

    fn sheet_path(&self, scale: u32) -> PathBuf {
        self.root
            .join("sheet")
            .join(variant_name("rowline_sheet", scale))
    }
}

impl AssetLoader for FileAssetLoader {
    fn load_sheet(
        &mut self,
        key: &AssetKey,
        grid: GridSpec,
        scale_factor: f32,
    ) -> Result<SpriteSheet, AssetError> {
        let available = self.available_scales(key);
        let want = rowline_display::select_variant(&available, scale_factor)
            .ok_or_else(|| AssetError::NotFound("no sprite sheet variant".into()))?;
        let path = self.sheet_path(want);
        let bytes =
            std::fs::read(&path).map_err(|e| AssetError::Io(format!("{}: {e}", path.display())))?;
        let img = decode_png(&bytes)?;
        Ok(SpriteSheet::new(img, grid, want))
    }

    fn load_bytes(&mut self, key: &AssetKey) -> Result<Vec<u8>, AssetError> {
        let path = match key {
            AssetKey::SpriteSheet => self.sheet_path(1),
            AssetKey::File(name) => self.root.join(name),
        };
        std::fs::read(&path).map_err(|e| AssetError::Io(format!("{}: {e}", path.display())))
    }

    fn available_scales(&self, _key: &AssetKey) -> Vec<u32> {
        let dir = self.root.join("sheet");
        let mut out = Vec::new();
        for s in 1..=4u32 {
            if dir.join(variant_name("rowline_sheet", s)).exists() {
                out.push(s);
            }
        }
        if out.is_empty() && dir.join("rowline_sheet.png").exists() {
            out.push(1);
        }
        out
    }
}

/// An in-memory loader, used by tests and by the head-less replay.
#[derive(Debug, Default, Clone)]
pub struct MemoryAssetLoader {
    files: BTreeMap<String, Vec<u8>>,
}

impl MemoryAssetLoader {
    /// An empty loader.
    pub fn new() -> Self {
        MemoryAssetLoader::default()
    }

    /// Add a file.
    pub fn insert(&mut self, name: &str, bytes: Vec<u8>) {
        self.files.insert(name.to_string(), bytes);
    }
}

impl AssetLoader for MemoryAssetLoader {
    fn load_sheet(
        &mut self,
        key: &AssetKey,
        grid: GridSpec,
        scale_factor: f32,
    ) -> Result<SpriteSheet, AssetError> {
        let available = self.available_scales(key);
        let want = rowline_display::select_variant(&available, scale_factor)
            .ok_or_else(|| AssetError::NotFound("no sprite sheet variant".into()))?;
        let name = format!("sheet/{}", variant_name("rowline_sheet", want));
        let bytes = self
            .files
            .get(&name)
            .ok_or_else(|| AssetError::NotFound(name.clone()))?;
        let img = decode_png(bytes)?;
        Ok(SpriteSheet::new(img, grid, want))
    }

    fn load_bytes(&mut self, key: &AssetKey) -> Result<Vec<u8>, AssetError> {
        let name = match key {
            AssetKey::SpriteSheet => "sheet/rowline_sheet.png".to_string(),
            AssetKey::File(n) => n.clone(),
        };
        self.files
            .get(&name)
            .cloned()
            .ok_or(AssetError::NotFound(name))
    }

    fn available_scales(&self, _key: &AssetKey) -> Vec<u32> {
        let mut out = Vec::new();
        for s in 1..=4u32 {
            let name = format!("sheet/{}", variant_name("rowline_sheet", s));
            if self.files.contains_key(&name) {
                out.push(s);
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ball_source_follows_the_index_contract() {
        let img = RgbaImage::new(576, 586);
        let sheet = SpriteSheet::new(img, GridSpec::ORIGINAL, 1);
        let sphere = sheet.ball(3, 0);
        assert_eq!(sphere.x, 108.0); // 3 * 36
        assert_eq!(sphere.y, 225.0);
        let polygon = sheet.ball(3, 1);
        assert_eq!(polygon.x, 396.0); // 3 * 36 + 288
        let empty = sheet.ball(0, 0);
        assert_eq!(empty.x, 0.0);
    }

    #[test]
    fn ball_source_scales_with_the_sheet() {
        let img = RgbaImage::new(1152, 1172);
        let sheet = SpriteSheet::new(img, GridSpec::ORIGINAL, 2);
        assert_eq!(sheet.ball(3, 0).x, 216.0);
        assert_eq!(sheet.ball(3, 0).y, 450.0);
        assert_eq!(sheet.ball(3, 0).width, 72.0);
    }

    #[test]
    fn variant_names_follow_the_convention() {
        assert_eq!(variant_name("rowline_sheet", 1), "rowline_sheet.png");
        assert_eq!(variant_name("rowline_sheet", 2), "rowline_sheet@2x.png");
    }

    #[test]
    fn lanczos_upscale_doubles_the_image() {
        let img = RgbaImage::from_pixel(4, 4, image::Rgba([10, 20, 30, 255]));
        let up = upscale_lanczos(&img, 2);
        assert_eq!(up.width(), 8);
        assert_eq!(up.height(), 8);
    }
}
