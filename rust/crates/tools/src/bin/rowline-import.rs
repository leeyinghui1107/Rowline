//! `wl-import` -- import the original's high-score table.
//!
//! The original stored its scores as a 512-byte binary blob in the Windows
//! registry under
//!
//! ```text
//! HKCU\SOFTWARE\Ivan Golubev\WinLinez\HighScore\Table
//! ```
//!
//! Export it on Windows (PowerShell, no extra tools needed):
//!
//! ```powershell
//! $k = Get-Item "HKCU:\SOFTWARE\Ivan Golubev\WinLinez\HighScore"
//! [Convert]::ToHexString($k.GetValue("Table"))
//! ```
//!
//! then feed the hex to this tool:
//!
//! ```text
//! wl-import --table <hex> --out save.ron
//! ```
//!
//! The obfuscated "Hash" value of the original is **not** imported: the
//! re-implementation uses a plain, versioned save format (docs/04, P-04).

use std::process::ExitCode;

use rowline_core::save::Settings;
use rowline_storage::legacy;

fn main() -> ExitCode {
    let mut table: Option<String> = None;
    let mut out = String::from("save.ron");
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--table" => table = args.next(),
            "--out" => out = args.next().unwrap_or(out),
            "--help" | "-h" => {
                println!("usage: wl-import --table <hex> [--out save.ron]");
                return ExitCode::SUCCESS;
            }
            other => {
                eprintln!("wl-import: unknown argument: {other}");
                return ExitCode::from(2);
            }
        }
    }

    let Some(hex) = table else {
        eprintln!("wl-import: --table <hex> is required");
        return ExitCode::from(2);
    };

    let records = match legacy::import_table_hex(&hex) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("wl-import: {e}");
            return ExitCode::FAILURE;
        }
    };

    let data = legacy::to_save(records.clone(), Settings::default());
    let format = rowline_storage::RonFormat;
    let bytes = match rowline_core::save::SaveFormat::encode(&format, &data) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("wl-import: {e}");
            return ExitCode::FAILURE;
        }
    };
    if let Err(e) = std::fs::write(&out, bytes) {
        eprintln!("wl-import: cannot write {out}: {e}");
        return ExitCode::FAILURE;
    }

    println!("wl-import: wrote {out}");
    for (i, r) in records.iter().enumerate().take(16) {
        if r.name.is_empty() && r.score == 0 {
            continue;
        }
        println!("  {:>2}. {:<28} {}", i + 1, r.name, r.score);
    }
    ExitCode::SUCCESS
}
