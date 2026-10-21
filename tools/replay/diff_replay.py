#!/usr/bin/env python3
"""
Differential replay harness -- the M2 acceptance gate.

Compares, for the same trace, the output of:
  * the Python reference implementation  (tools/replay/model.py)
  * the C implementation                (build/rowline-replay)

and optionally a recorded *expected* result (golden file).

Usage:
    python3 tools/replay/diff_replay.py --trace cases/x.trace
    python3 tools/replay/diff_replay.py --trace cases/x.trace --golden cases/x.golden
    python3 tools/replay/diff_replay.py --all          # run every case in cases/
    python3 tools/replay/diff_replay.py --record cases/x.trace   # create golden

Exit code 0 = all comparisons match, 1 = divergence found.
"""
import argparse, difflib, os, subprocess, sys, glob

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, "..", ".."))
sys.path.insert(0, HERE)
import model as M

def _find(rel):
    """The binary, with or without the Windows executable suffix."""
    base = os.path.join(ROOT, rel)
    for cand in (base, base + ".exe"):
        if os.path.exists(cand):
            return cand
    return base


C_BIN = _find(os.path.join("build", "rowline-replay"))
RUST_BIN = _find(os.path.join("rust", "target", "release", "rowline-replay"))


def run_python(seed, board, ops):
    eng = M.run_trace(seed, board, ops)
    return eng.dump()


def _run(binary, trace_path):
    if not os.path.exists(binary):
        return None, "not built (%s missing)" % os.path.relpath(binary, ROOT)
    try:
        p = subprocess.run([binary, trace_path], capture_output=True, text=True, timeout=30)
    except Exception as e:
        return None, "failed to run: %s" % e
    if p.returncode != 0:
        return None, "exited %d: %s" % (p.returncode, p.stderr.strip())
    return p.stdout.strip(), None


def run_c(trace_path):
    return _run(C_BIN, trace_path)


def run_rust(trace_path):
    return _run(RUST_BIN, trace_path)


def diff(a, b, la, lb):
    d = list(difflib.unified_diff(a.splitlines(), b.splitlines(),
                                  fromfile=la, tofile=lb, lineterm=""))
    return "\n".join(d)


def compare(trace_path, golden_path=None, verbose=False):
    seed, board, ops = M.load_trace(trace_path)
    py = run_python(seed, board, ops)
    results = [("python-reference", py)]

    c_out, cerr = run_c(trace_path)
    results.append(("c-implementation",
                    "<error: %s>" % cerr if cerr else c_out))

    r_out, rerr = run_rust(trace_path)
    results.append(("rust-implementation",
                    "<error: %s>" % rerr if rerr else r_out))

    if golden_path and os.path.exists(golden_path):
        results.append(("golden", open(golden_path).read().strip()))

    ok = True
    report = []
    base = results[0][1].splitlines()
    for name, out in results[1:]:
        if out.splitlines() == base:
            report.append("  OK   %-20s matches python-reference" % name)
        else:
            ok = False
            report.append("  DIFF %-20s diverges from python-reference" % name)
            report.append(diff("\n".join(base), out, "python-reference", name))
    if len(results) == 1:
        report.append("  (only python-reference available - build the C impl to compare)")
    return ok, "\n".join(report)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--trace")
    ap.add_argument("--golden")
    ap.add_argument("--all", action="store_true")
    ap.add_argument("--record", nargs="?", const="",
                    help="write the python-reference output as golden (optionally to this path)")
    ap.add_argument("-v", "--verbose", action="store_true")
    a = ap.parse_args()

    traces = []
    if a.all:
        traces = sorted(glob.glob(os.path.join(ROOT, "tools", "replay", "cases", "*.trace")))
    elif a.trace:
        traces = [a.trace]
    if not traces:
        ap.error("need --trace or --all")

    if a.record is not None:
        if a.record is not None:
            golden = a.record
        for t in traces:
            g = golden if (a.record and len(traces) == 1) else os.path.splitext(t)[0] + ".golden"
            seed, board, ops = M.load_trace(t)
            open(g, "w").write(run_python(seed, board, ops) + "\n")
            print("recorded golden -> %s" % g)
        return 0

    allok = True
    for t in traces:
        golden = a.golden
        if golden is None:
            golden = os.path.splitext(t)[0] + ".golden"
        if a.record is not None:
            seed, board, ops = M.load_trace(t)
            open(golden, "w").write(run_python(seed, board, ops) + "\n")
            print("recorded golden -> %s" % golden)
            continue
        print("=== %s" % os.path.relpath(t, ROOT))
        ok, rep = compare(t, golden, a.verbose)
        print(rep)
        allok = allok and ok

    if not a.record:
        print()
        print("RESULT: %s" % ("ALL MATCH" if allok else "DIVERGENCE DETECTED"))
    return 0 if allok else 1


if __name__ == "__main__":
    sys.exit(main())
