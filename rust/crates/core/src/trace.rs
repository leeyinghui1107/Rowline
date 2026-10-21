//! The replay trace format.
//!
//! Deliberately line-based TSV so that the Python, C and Rust
//! implementations can all parse it without a JSON dependency, and so that a
//! human can read and hand-edit a trace.
//!
//! ```text
//! seed   <u32>
//! board  <81 chars>          optional; 0-7 = colour, '.' = empty
//! op move <r0> <c0> <r1> <c1>
//! op click <r> <c>
//! op newgame
//! op setusefigures <0|1>
//! ```

use crate::board::Board;

/// A single recorded operation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Op {
    Move { r0: i32, c0: i32, r1: i32, c1: i32 },
    Click { r: i32, c: i32 },
    NewGame,
    SetUseFigures(u8),
}

/// A parsed trace.
#[derive(Clone, Debug, Default)]
pub struct Trace {
    pub seed: u32,
    pub board: Option<Board>,
    pub ops: Vec<Op>,
}

/// Why a trace could not be parsed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TraceError(pub String);

impl std::fmt::Display for TraceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "trace error: {}", self.0)
    }
}

impl std::error::Error for TraceError {}

impl Trace {
    /// Parse a trace from text.
    pub fn parse(text: &str) -> Result<Trace, TraceError> {
        let mut t = Trace::default();
        for raw in text.lines() {
            let line = raw.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let mut parts = line.split_whitespace();
            let key = parts.next().unwrap_or("");
            match key {
                "seed" => {
                    let v = parts
                        .next()
                        .ok_or_else(|| TraceError("seed needs a value".into()))?;
                    t.seed = parse_u32(v)?;
                }
                "board" => {
                    let v = parts
                        .next()
                        .ok_or_else(|| TraceError("board needs a value".into()))?;
                    t.board = Some(
                        Board::from_trace_string(v)
                            .ok_or_else(|| TraceError(format!("bad board string: {v}")))?,
                    );
                }
                "op" => {
                    let kind = parts
                        .next()
                        .ok_or_else(|| TraceError("op needs a kind".into()))?;
                    let mut nums = Vec::new();
                    for p in parts {
                        nums.push(parse_int(p)?);
                    }
                    let need = |n: usize| -> Result<(), TraceError> {
                        if nums.len() >= n {
                            Ok(())
                        } else {
                            Err(TraceError(format!("op {kind} needs {n} arguments")))
                        }
                    };
                    let op = match kind {
                        "move" => {
                            need(4)?;
                            Op::Move {
                                r0: nums[0],
                                c0: nums[1],
                                r1: nums[2],
                                c1: nums[3],
                            }
                        }
                        "click" => {
                            need(2)?;
                            Op::Click {
                                r: nums[0],
                                c: nums[1],
                            }
                        }
                        "newgame" => Op::NewGame,
                        "setusefigures" => {
                            need(1)?;
                            Op::SetUseFigures(nums[0] as u8)
                        }
                        other => return Err(TraceError(format!("unknown op: {other}"))),
                    };
                    t.ops.push(op);
                }
                other => return Err(TraceError(format!("unknown key: {other}"))),
            }
        }
        Ok(t)
    }
}

fn parse_int(s: &str) -> Result<i32, TraceError> {
    let s = s.trim();
    if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        i32::from_str_radix(hex, 16).map_err(|_| TraceError(format!("bad number: {s}")))
    } else {
        s.parse::<i32>()
            .map_err(|_| TraceError(format!("bad number: {s}")))
    }
}

/// Seeds are unsigned 32 bit and may exceed `i32::MAX` (e.g. 0xDEADBEEF),
/// so they need their own parser.
fn parse_u32(s: &str) -> Result<u32, TraceError> {
    let s = s.trim();
    if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        u32::from_str_radix(hex, 16).map_err(|_| TraceError(format!("bad seed: {s}")))
    } else {
        s.parse::<u32>()
            .map_err(|_| TraceError(format!("bad seed: {s}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_minimal_trace() {
        let t = Trace::parse("seed 42\nop move 0 0 1 1\n").unwrap();
        assert_eq!(t.seed, 42);
        assert_eq!(t.ops.len(), 1);
        assert_eq!(
            t.ops[0],
            Op::Move {
                r0: 0,
                c0: 0,
                r1: 1,
                c1: 1
            }
        );
    }

    #[test]
    fn rejects_unknown_ops() {
        assert!(Trace::parse("op teleport 1 2").is_err());
    }
}
