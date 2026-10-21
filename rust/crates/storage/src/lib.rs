//! Persistence: the save format, the on-disk store and the legacy importer.
//!
//! Three pieces:
//!
//! * [`RonFormat`] -- the readable save encoding.  RON was chosen over
//!   JSON because it keeps comments and is hand-editable, and over a binary
//!   format because a player should be able to look at their own save.
//! * [`Store`] -- reads and writes the save through the platform's
//!   [`FileSystem`], so it works against the real disk *and* against the
//!   in-memory file system used by the tests.
//! * [`legacy`] -- the importer for the original's 512-byte registry table.
//!
//! The original's obfuscated "Hash" value is **not** reproduced (see
//! `docs/04` decision P-04): the re-implementation uses a plain, versioned
//! format and only imports the score table.

use std::path::{Path, PathBuf};

use rowline_core::save::{SaveData, SaveError, SaveFormat, ScoreRecord, Settings};
use rowline_platform::{FileSystem, IoError};

/// The save encoding: RON.
#[derive(Clone, Copy, Debug, Default)]
pub struct RonFormat;

impl SaveFormat for RonFormat {
    fn encode(&self, save: &SaveData) -> Result<Vec<u8>, SaveError> {
        let text = ron::ser::to_string_pretty(save, ron::ser::PrettyConfig::default())
            .map_err(|e| SaveError::Encode(e.to_string()))?;
        Ok(text.into_bytes())
    }

    fn decode(&self, bytes: &[u8]) -> Result<SaveData, SaveError> {
        let text = std::str::from_utf8(bytes).map_err(|e| SaveError::Decode(e.to_string()))?;
        ron::from_str(text).map_err(|e| SaveError::Decode(e.to_string()))
    }

    fn extension(&self) -> &'static str {
        "ron"
    }
}

/// The store: the save file inside the application's data directory.
pub struct Store<'a, F: FileSystem> {
    fs: &'a mut F,
    format: RonFormat,
    path: Option<PathBuf>,
}

impl<'a, F: FileSystem> Store<'a, F> {
    /// A store writing to `<app_dir>/save.ron`.
    pub fn new(fs: &'a mut F) -> Self {
        let path = fs.app_dir().join("save.ron");
        Store {
            fs,
            format: RonFormat,
            path: Some(path),
        }
    }

    /// A store writing to an explicit path.
    pub fn at(fs: &'a mut F, path: impl Into<PathBuf>) -> Self {
        Store {
            fs,
            format: RonFormat,
            path: Some(path.into()),
        }
    }

    /// The path the store writes to.
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// Load the save.
    ///
    /// A missing file is **not** an error: it yields the defaults.
    pub fn load(&mut self) -> Result<SaveData, SaveError> {
        let Some(path) = self.path.clone() else {
            return Ok(SaveData::default());
        };
        match self.fs.read(&path) {
            Ok(bytes) => self.format.decode(&bytes),
            Err(IoError::NotFound(_)) => Ok(SaveData::default()),
            Err(e) => Err(SaveError::Io(e.to_string())),
        }
    }

    /// Save the data.
    pub fn save(&mut self, data: &SaveData) -> Result<(), SaveError> {
        let Some(path) = self.path.clone() else {
            return Err(SaveError::Io("no path configured".into()));
        };
        let bytes = self.format.encode(data)?;
        self.fs
            .write(&path, &bytes)
            .map_err(|e| SaveError::Io(e.to_string()))
    }
}

/// Importing the original's data.
pub mod legacy {
    use super::*;

    /// The original's score table, as a hex string (512 bytes).
    ///
    /// The user exports it on Windows with, for example:
    ///
    /// ```text
    /// reg export "HKCU\SOFTWARE\Ivan Golubev\WinLinez\HighScore" out.reg
    /// ```
    ///
    /// and then extracts the `Table` value's bytes.
    pub fn import_table_hex(hex: &str) -> Result<Vec<ScoreRecord>, SaveError> {
        let cleaned: String = hex.chars().filter(|c| c.is_ascii_hexdigit()).collect();
        if cleaned.len() % 2 != 0 {
            return Err(SaveError::Decode("odd number of hex digits".into()));
        }
        let bytes: Vec<u8> = (0..cleaned.len() / 2)
            .map(|i| u8::from_str_radix(&cleaned[i * 2..i * 2 + 2], 16))
            .collect::<Result<Vec<u8>, _>>()
            .map_err(|e| SaveError::Decode(e.to_string()))?;
        if bytes.len() != rowline_core::save::legacy::TABLE_LEN {
            return Err(SaveError::Invalid(format!(
                "expected {} bytes, got {}",
                rowline_core::save::legacy::TABLE_LEN,
                bytes.len()
            )));
        }
        Ok(rowline_core::save::legacy::decode_table(&bytes))
    }

    /// Build a save file from an imported table.
    pub fn to_save(records: Vec<ScoreRecord>, settings: Settings) -> SaveData {
        SaveData {
            version: 1,
            scores: records,
            settings,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rowline_platform::MemoryFileSystem;

    #[test]
    fn ron_round_trips() {
        let f = RonFormat;
        let mut data = SaveData::default();
        data.version = 1;
        data.scores.push(ScoreRecord {
            name: "ACE".into(),
            score: 1234,
        });
        data.settings.theme = 1;
        data.settings.text_scale = 3;
        let bytes = f.encode(&data).unwrap();
        let back = f.decode(&bytes).unwrap();
        assert_eq!(back, data);
    }

    #[test]
    fn the_extension_is_ron() {
        assert_eq!(RonFormat.extension(), "ron");
    }

    #[test]
    fn a_missing_save_yields_the_defaults() {
        let mut fs = MemoryFileSystem::new();
        let mut store = Store::new(&mut fs);
        let data = store.load().unwrap();
        assert_eq!(data.version, 0);
        assert!(data.scores.is_empty());
    }

    #[test]
    fn a_save_round_trips_through_the_store() {
        let mut fs = MemoryFileSystem::new();
        let mut data = SaveData::default();
        data.scores.push(ScoreRecord {
            name: "X".into(),
            score: 7,
        });
        {
            let mut store = Store::new(&mut fs);
            store.save(&data).unwrap();
        }
        let mut store = Store::new(&mut fs);
        assert_eq!(store.load().unwrap(), data);
    }

    #[test]
    fn the_legacy_importer_validates_the_length() {
        assert!(legacy::import_table_hex("00").is_err());
        assert!(legacy::import_table_hex("zz").is_err());
        let good = "00".repeat(512);
        let recs = legacy::import_table_hex(&good).unwrap();
        assert_eq!(recs.len(), 16);
        assert_eq!(recs[0].name, "");
        assert_eq!(recs[0].score, 0);
    }

    #[test]
    fn the_legacy_importer_reads_names_and_scores() {
        let mut blob = vec![0u8; 512];
        blob[0..3].copy_from_slice(b"ACE");
        blob[0x1C..0x20].copy_from_slice(&1000u32.to_le_bytes());
        let recs =
            legacy::import_table_hex(&blob.iter().map(|b| format!("{b:02x}")).collect::<String>())
                .unwrap();
        assert_eq!(recs[0].name, "ACE");
        assert_eq!(recs[0].score, 1000);
        assert_eq!(recs[1].score, 0);
    }
}
