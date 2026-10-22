//! Assets compiled into the executable.
//!
//! The product is a single file: the sprite sheet, the icon strip and the UI
//! font are all embedded with `include_bytes!`.  There is no asset
//! directory to copy, no runtime file-system lookup and no dependency on an
//! installed system font.

use image::RgbaImage;

/// Something that went wrong while decoding an embedded asset.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AssetError {
    /// The asset does not exist.
    NotFound(String),
    /// The image could not be decoded.
    Decode(String),
}

impl std::fmt::Display for AssetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AssetError::NotFound(k) => write!(f, "asset not found: {k}"),
            AssetError::Decode(m) => write!(f, "asset decode failed: {m}"),
        }
    }
}

impl std::error::Error for AssetError {}

/// A decoded sprite sheet.
#[derive(Clone, Debug)]
pub struct SpriteSheet {
    image: RgbaImage,
    /// The integer scale this sheet was authored at (1, 2, 3, ...).
    scale: u32,
}

impl SpriteSheet {
    /// Wrap an image with the integer scale it was authored at.
    pub fn new(image: RgbaImage, scale: u32) -> Self {
        SpriteSheet {
            image,
            scale: scale.max(1),
        }
    }

    /// The integer scale of the sheet.
    pub fn scale(&self) -> u32 {
        self.scale
    }

    /// The underlying image.
    pub fn image(&self) -> &RgbaImage {
        &self.image
    }
}

/// The sprite-sheet variants compiled into the binary.
pub const EMBEDDED_SHEET_SCALES: [u32; 4] = [1, 2, 3, 4];

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

/// The UI font compiled into the binary.
///
/// WenQuanYi Micro Hei (Apache-2.0, see `assets/font/NOTICE.txt`) covers
/// Latin and CJK, so the menu, HUD and name entry render identically on a
/// machine with no CJK system font.
pub fn embedded_font() -> &'static [u8] {
    include_bytes!("../../../../assets/font/wqy-microhei.ttc")
}

/// Decode an embedded sprite sheet.
pub fn embedded_sheet_image(scale: u32) -> Result<SpriteSheet, AssetError> {
    let bytes = embedded_sheet(scale)
        .ok_or_else(|| AssetError::NotFound(format!("embedded sheet @{scale}x")))?;
    let image = decode_png(bytes)?;
    Ok(SpriteSheet::new(image, scale))
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

/// Decode a PNG file.
pub fn decode_png(bytes: &[u8]) -> Result<RgbaImage, AssetError> {
    image::load_from_memory_with_format(bytes, image::ImageFormat::Png)
        .map(|img| img.to_rgba8())
        .map_err(|e| AssetError::Decode(e.to_string()))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_embedded_sheet_decodes() {
        let sheet = embedded_sheet_image(1).unwrap();
        assert_eq!(sheet.scale(), 1);
        assert!(sheet.image().width() > 0);
    }

    #[test]
    fn the_embedded_font_is_present() {
        assert!(embedded_font().len() > 1000);
    }
}
