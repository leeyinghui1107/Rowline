//! `wl-replay` -- head-less replay CLI.
//!
//! Reads a trace (see `rowline_core::trace`) and prints the result events.
//! The output must match `tools/replay/model.py` and `build/rowline-replay`
//! byte for byte; `tools/replay/diff_replay.py` compares all three.
//!
//! ```text
//! wl-replay <trace-file>
//! ```

use std::fmt::Write as _;
use std::process::ExitCode;

use rowline_core::engine::MoveStatus;
use rowline_core::{coords, Engine, Op, RuleModel, Trace};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("usage: {} <trace-file>", args[0]);
        return ExitCode::from(2);
    }
    let text = match std::fs::read_to_string(&args[1]) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("cannot read {}: {e}", args[1]);
            return ExitCode::from(2);
        }
    };
    let trace = match Trace::parse(&text) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::from(2);
        }
    };

    let mut eng = Engine::new(trace.seed, RuleModel::default());
    let mut out = String::new();

    match &trace.board {
        Some(b) => {
            *eng.board_mut() = b.clone();
        }
        None => {
            eng.new_game();
            let _ = writeln!(
                out,
                "state {} {} {}",
                eng.step(),
                eng.board().to_trace_string(),
                eng.score()
            );
        }
    }

    for op in &trace.ops {
        eng.bump();
        match op {
            Op::Move { r0, c0, r1, c1 } => {
                let res = eng.move_ball(*r0, *c0, *r1, *c1);
                match res.status {
                    Some(MoveStatus::Illegal) => {
                        let _ = writeln!(out, "note {} illegal move", eng.step());
                    }
                    Some(MoveStatus::NoPath) => {
                        let _ = writeln!(out, "note {} no path", eng.step());
                    }
                    Some(MoveStatus::Moved) => {
                        let _ = write!(out, "path {}", eng.step());
                        for &p in &res.path {
                            let (r, c) = coords(p);
                            let _ = write!(out, " {r},{c}");
                        }
                        let _ = writeln!(out);
                        if !res.group.is_empty() {
                            let _ = write!(out, "elim {} {}", eng.step(), res.group.len());
                            for &g in &res.group {
                                let (r, c) = coords(g);
                                let _ = write!(out, " {r},{c}");
                            }
                            let _ = writeln!(out, " {}", res.delta);
                        }
                        let _ = writeln!(
                            out,
                            "state {} {} {}",
                            eng.step(),
                            eng.board().to_trace_string(),
                            eng.score()
                        );
                    }
                    None => {}
                }
            }
            Op::Click { r, c } => {
                let _ = writeln!(out, "note {} click {r},{c}", eng.step());
            }
            Op::NewGame => {
                eng.new_game();
                let _ = writeln!(
                    out,
                    "state {} {} {}",
                    eng.step(),
                    eng.board().to_trace_string(),
                    eng.score()
                );
            }
            Op::SetUseFigures(v) => {
                // The polygon sprite set was removed from the product.  The
                // trace op is kept as an inert note so old recorded traces
                // still replay and the four-way gate keeps its fixtures.
                let _ = writeln!(out, "note {} usefigures {v}", eng.step());
            }
        }
    }

    print!("{out}");
    ExitCode::SUCCESS
}
