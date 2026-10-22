//! Persistence: the save format, the on-disk store and the legacy importer.
//!
//! Three pieces:
//!
//! * [`TextFormat`] -- a small, hand-editable text encoding.  It replaces
//!   a generic serialisation crate with ~150 lines of parser, which keeps
//!   the executable small while the file stays readable.
//! * [`Store`] -- reads and writes the save through the platform's
//!   [`FileSystem`], so it works against the real disk *and* against the
//!   in-memory file system used by the tests.
//! * [`legacy`] -- the importer for the original's 512-byte registry table.
//!
//! The original's obfuscated "Hash" value is **not** reproduced (see
//! `docs/04` decision P-04): the re-implementation uses a plain, versioned
//! format and only imports the score table.

use std::fmt::Write as _;
use std::path::PathBuf;

use rowline_core::save::{SaveData, SaveError, SaveFormat, ScoreRecord, Settings};
use rowline_platform::{FileSystem, IoError};

/// The save encoding: a small, line-oriented text format.
///
/// A player can open the file in any editor.  It is deliberately simpler
/// than a general serialisation format (no external crates): the save is
/// twelve settings and a score table, and a hand-written parser keeps the
/// executable small and the format obvious.
///
/// ```text
/// rowline-save 1
/// score "国王" 100
/// settings window_x=478 window_y=324 window_width=604 window_height=396 ...
/// ```
#[derive(Clone, Copy, Debug, Default)]
pub struct TextFormat;

impl SaveFormat for TextFormat {
    fn encode(&self, save: &SaveData) -> Result<Vec<u8>, SaveError> {
        let mut out = String::new();
        let _ = writeln!(out, "rowline-save {}", save.version);
        for row in &save.scores {
            let _ = writeln!(out, "score {} {}", quote(&row.name), row.score);
        }
        let s = &save.settings;
        let _ = writeln!(
            out,
            concat!("settings window_x={} window_y={} window_width={} window_height={}"),
            s.window_x, s.window_y, s.window_width, s.window_height
        );
        Ok(out.into_bytes())
    }

    fn decode(&self, bytes: &[u8]) -> Result<SaveData, SaveError> {
        let text = std::str::from_utf8(bytes).map_err(|e| SaveError::Decode(e.to_string()))?;
        let mut save = SaveData::default();
        let mut header = false;
        for (n, raw) in text.lines().enumerate() {
            let line = raw.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let bad = |what: &str| SaveError::Decode(format!("line {}: {what}", n + 1));
            if let Some(rest) = line.strip_prefix("rowline-save ") {
                save.version = rest.trim().parse().map_err(|_| bad("bad format version"))?;
                header = true;
            } else if let Some(rest) = line.strip_prefix("score ") {
                let (name, score) = parse_score(rest).map_err(|e| bad(&e))?;
                save.scores.push(ScoreRecord { name, score });
            } else if let Some(rest) = line.strip_prefix("settings ") {
                for pair in rest.split_whitespace() {
                    let Some((key, value)) = pair.split_once('=') else {
                        return Err(bad("bad settings entry"));
                    };
                    apply_setting(&mut save.settings, key, value).map_err(|e| bad(&e))?;
                }
            } else {
                return Err(bad("unknown record"));
            }
        }
        if !header {
            return Err(SaveError::Decode("missing rowline-save header".into()));
        }
        Ok(save)
    }

    fn extension(&self) -> &'static str {
        "txt"
    }
}

/// Quote a player name so spaces and quotes survive a round trip.
fn quote(name: &str) -> String {
    let mut out = String::with_capacity(name.len() + 2);
    out.push('"');
    for ch in name.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            _ => out.push(ch),
        }
    }
    out.push('"');
    out
}

/// Parse `"name" 1234` (or an unquoted `name 1234`).
fn parse_score(rest: &str) -> Result<(String, u32), String> {
    let rest = rest.trim();
    let (name, tail) = if let Some(body) = rest.strip_prefix('"') {
        let mut name = String::new();
        let mut chars = body.chars();
        let mut closed = false;
        while let Some(ch) = chars.next() {
            match ch {
                '\\' => match chars.next() {
                    Some('"') => name.push('"'),
                    Some('\\') => name.push('\\'),
                    Some('n') => name.push('\n'),
                    Some('r') => name.push('\r'),
                    Some('t') => name.push('\t'),
                    _ => return Err("bad escape in name".into()),
                },
                '"' => {
                    closed = true;
                    break;
                }
                _ => name.push(ch),
            }
        }
        if !closed {
            return Err("unterminated name".into());
        }
        let tail: String = chars.collect();
        (name, tail)
    } else {
        let (name, tail) = rest
            .split_once(char::is_whitespace)
            .ok_or("missing score")?;
        (name.to_string(), tail.to_string())
    };
    let score: u32 = tail.trim().parse().map_err(|_| "bad score".to_string())?;
    Ok((name, score))
}

fn apply_setting(s: &mut Settings, key: &str, value: &str) -> Result<(), String> {
    let int = |v: &str| v.parse::<i32>().map_err(|_| format!("bad {key}"));
    let uint = |v: &str| v.parse::<u32>().map_err(|_| format!("bad {key}"));
    match key {
        "window_x" => s.window_x = int(value)?,
        "window_y" => s.window_y = int(value)?,
        "window_width" => s.window_width = uint(value)?,
        "window_height" => s.window_height = uint(value)?,
        // Unknown keys are ignored, so a newer build can add settings
        // without older ones refusing to load the file.
        _ => {}
    }
    Ok(())
}

/// The store: the save file inside the application's data directory.
pub struct Store<'a, F: FileSystem> {
    fs: &'a mut F,
    format: TextFormat,
    path: Option<PathBuf>,
}

impl<'a, F: FileSystem> Store<'a, F> {
    /// A store writing to `<app_dir>/save.txt`.
    pub fn new(fs: &'a mut F) -> Self {
        let path = fs.app_dir().join("save.txt");
        Store {
            fs,
            format: TextFormat,
            path: Some(path),
        }
    }

    /// A store writing to an explicit path.
    pub fn at(fs: &'a mut F, path: impl Into<PathBuf>) -> Self {
        Store {
            fs,
            format: TextFormat,
            path: Some(path.into()),
        }
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
        if !cleaned.len().is_multiple_of(2) {
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
            version: rowline_core::save::SAVE_VERSION,
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
    fn the_text_format_round_trips() {
        let f = TextFormat;
        let mut data = SaveData {
            version: 1,
            ..SaveData::default()
        };
        data.scores.push(ScoreRecord {
            name: "ACE".into(),
            score: 1234,
        });
        data.scores.push(ScoreRecord {
            name: "a \"quoted\" name".into(),
            score: 7,
        });
        data.settings.window_x = -1;
        let bytes = f.encode(&data).unwrap();
        let back = f.decode(&bytes).unwrap();
        assert_eq!(back, data);
    }

    #[test]
    fn the_extension_is_txt() {
        assert_eq!(TextFormat.extension(), "txt");
    }

    #[test]
    fn a_save_without_a_header_is_rejected() {
        assert!(TextFormat.decode(b"score x 1\n").is_err());
        assert!(TextFormat
            .decode(b"rowline-save 1\nunknown thing\n")
            .is_err());
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
