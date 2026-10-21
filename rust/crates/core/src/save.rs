//! Persistence: the save model and the pluggable encoding.
//!
//! The original stored its high-score table as a 512-byte binary blob in the
//! Windows registry, protected by a 16-byte XOR-obfuscated digest.  The
//! re-implementation keeps the *layout* (16 records of 32 bytes, a 28-byte
//! NUL-padded name and a u32 score at +0x1C) so that old scores can be
//! imported, but replaces the encoding with a readable, versioned format
//! (see docs/04 decision P-04: the obfuscation is not reproduced).
//!
//! The encoding itself is pluggable through [`SaveFormat`], so the same
//! model can be written as RON, JSON, or a compact binary blob.

/// One high-score entry.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ScoreRecord {
    /// Player name, at most 28 bytes in the legacy layout.
    pub name: String,
    /// Points.
    pub score: u32,
}

/// The persisted settings.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Settings {
    pub window_x: i32,
    pub window_y: i32,
    pub window_width: u32,
    pub window_height: u32,
    pub window_maximized: bool,
    pub show_next: bool,
    pub show_statistics: bool,
    pub use_figures: bool,
    /// The colour theme: `0` = dark, `1` = light.
    pub theme: u8,
    /// `true` when the user asked for reduced motion.
    pub reduced_motion: bool,
    /// The text size preference: `0..=3`.
    pub text_scale: u8,
    /// `true` when the fixed-resolution ("retro") scaling policy is on.
    pub retro_mode: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            window_x: 0,
            window_y: 0,
            window_width: 1280,
            window_height: 960,
            window_maximized: false,
            show_next: true,
            show_statistics: true,
            use_figures: false,
            theme: 0,
            reduced_motion: false,
            text_scale: 1,
            retro_mode: false,
        }
    }
}

/// Everything that gets persisted.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SaveData {
    /// Format version, so future readers can migrate.
    pub version: u32,
    /// The score table, ordered best first.
    pub scores: Vec<ScoreRecord>,
    /// The settings.
    pub settings: Settings,
}

/// Everything that can go wrong while saving or loading.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SaveError {
    /// The bytes could not be parsed.
    Decode(String),
    /// The data could not be serialised.
    Encode(String),
    /// The file could not be read or written.
    Io(String),
    /// The data is structurally valid but semantically wrong.
    Invalid(String),
}

impl std::fmt::Display for SaveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SaveError::Decode(m) => write!(f, "save decode failed: {m}"),
            SaveError::Encode(m) => write!(f, "save encode failed: {m}"),
            SaveError::Io(m) => write!(f, "save I/O failed: {m}"),
            SaveError::Invalid(m) => write!(f, "save data invalid: {m}"),
        }
    }
}

impl std::error::Error for SaveError {}

/// A pluggable save encoding.
pub trait SaveFormat {
    /// Serialise the save data.
    fn encode(&self, save: &SaveData) -> Result<Vec<u8>, SaveError>;
    /// Deserialise the save data.
    fn decode(&self, bytes: &[u8]) -> Result<SaveData, SaveError>;
    /// The file extension this format uses, without the dot.
    fn extension(&self) -> &'static str;
}

/// Legacy constants of the original's binary table, kept so that the
/// importer can reuse them.
pub mod legacy {
    use super::ScoreRecord;

    /// Entries in the table.
    pub const ENTRIES: usize = 16;
    /// Bytes per entry.
    pub const RECORD_LEN: usize = 32;
    /// Bytes of the NUL-padded name field.
    pub const NAME_LEN: usize = 28;
    /// Offset of the u32 score inside a record.
    pub const SCORE_OFFSET: usize = 0x1C;
    /// Total table length.
    pub const TABLE_LEN: usize = ENTRIES * RECORD_LEN;

    /// Decode a legacy 512-byte table into records.
    pub fn decode_table(blob: &[u8]) -> Vec<ScoreRecord> {
        let mut out = Vec::with_capacity(ENTRIES);
        for i in 0..ENTRIES {
            let base = i * RECORD_LEN;
            if base + RECORD_LEN > blob.len() {
                break;
            }
            let raw = &blob[base..base + NAME_LEN];
            let end = raw.iter().position(|&b| b == 0).unwrap_or(raw.len());
            let name = String::from_utf8_lossy(&raw[..end]).to_string();
            let score = u32::from_le_bytes([
                blob[base + SCORE_OFFSET],
                blob[base + SCORE_OFFSET + 1],
                blob[base + SCORE_OFFSET + 2],
                blob[base + SCORE_OFFSET + 3],
            ]);
            out.push(ScoreRecord { name, score });
        }
        out
    }

    /// Encode records back into the legacy 512-byte table.
    pub fn encode_table(records: &[ScoreRecord]) -> Vec<u8> {
        let mut blob = vec![0u8; TABLE_LEN];
        for (i, r) in records.iter().take(ENTRIES).enumerate() {
            let base = i * RECORD_LEN;
            let bytes = r.name.as_bytes();
            let n = bytes.len().min(NAME_LEN);
            blob[base..base + n].copy_from_slice(&bytes[..n]);
            blob[base + SCORE_OFFSET..base + SCORE_OFFSET + 4]
                .copy_from_slice(&r.score.to_le_bytes());
        }
        blob
    }
}

/// Build the save data that corresponds to the current settings and scores.
pub fn capture(settings: Settings, scores: Vec<ScoreRecord>) -> SaveData {
    SaveData {
        version: 1,
        scores,
        settings,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_table_round_trips() {
        let records = vec![
            ScoreRecord {
                name: "AAA".into(),
                score: 100,
            },
            ScoreRecord {
                name: "BBB".into(),
                score: 250,
            },
        ];
        let blob = legacy::encode_table(&records);
        assert_eq!(blob.len(), legacy::TABLE_LEN);
        let back = legacy::decode_table(&blob);
        assert_eq!(back[0].name, "AAA");
        assert_eq!(back[0].score, 100);
        assert_eq!(back[1].score, 250);
    }

    #[test]
    fn long_names_are_truncated_not_panicking() {
        let records = vec![ScoreRecord {
            name: "X".repeat(100),
            score: 1,
        }];
        let blob = legacy::encode_table(&records);
        assert_eq!(blob.len(), legacy::TABLE_LEN);
        assert_eq!(legacy::decode_table(&blob)[0].name.len(), legacy::NAME_LEN);
    }
}
