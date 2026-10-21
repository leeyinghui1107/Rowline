#!/usr/bin/env python3
"""
Conformance analysis for the black-box phase.

Takes the artefacts produced by the collectors and compares them against the
static-analysis expectations (docs/03_FEATURE_SPEC.md), then emits a
conformance report and can auto-close the checklist items that the machine
evidence settles.

Sub-commands
    shot  <png> [--out dir]   measure a screenshot, compare with the formulas
    probe <dir>               compare a winprobe output directory
    close <id> <evidence>     mark a checklist item done (with the evidence)

Examples
    python3 tools/blackbox/analyze.py shot  shots/board.png --out blackbox/run1
    python3 tools/blackbox/analyze.py probe blackbox/run1
"""
import argparse, json, os, subprocess, sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, "..", ".."))
sys.path.insert(0, HERE)
sys.path.insert(0, os.path.join(ROOT, "tools"))
import geometry
import pngio

INSET = geometry.INSET

# ---------------------------------------------------------------- spec values
SPEC = {
    "cell_px": 36,
    "sprite_px": 32,
    "inset": 2,
    "board_frame_w": 327,
    "board_frame_h": 328,
    "panel_w": 125,
    "board_side": 9,
    "colors": 7,
    "threshold": 5,
    "ini_keys": ["WindowX", "WindowY", "WindowWidth", "WindowHeight",
                 "WindowMaximized", "ShowNext", "ShowStatistics", "UseFigures"],
    "score_entries": 16,
    "score_record_len": 32,
    "score_name_len": 28,
    "score_offset": 0x1C,
    "score_table_len": 512,
    "hash_len": 16,
    "menu_ids": [256, 257, 258, 512, 513, 514, 768, 769, 16528],
    "window_class": "WinLinezWindowClass",
    # reference palette recovered from the original atlas histogram.
    # Used only to NAME a measured colour, never to decide what was drawn.
    "reference_palette": [(0, 160, 0), (0, 0, 192), (0, 192, 192),
                          (192, 0, 192), (192, 0, 0), (192, 192, 0),
                          (128, 0, 0)],
}
PALETTE_NAMES = ["GREEN", "BLUE", "CYAN", "MAGENTA", "RED", "YELLOW", "MAROON"]


def idiv2(v):
    return -((-v + 1)//2) if v < 0 else v//2


def spec_layout(w, h):
    board_x = idiv2(w - 108)
    board_y = idiv2(h - 234) + 234
    t = idiv2(w - 328)
    panel_x = t + 328 + idiv2(t - 58)
    return board_x, board_y, panel_x


# ---------------------------------------------------------------- colours
def slot_colour(rows, ox, oy, r, c):
    """Dominant saturated colour of one cell, or None when the cell is empty."""
    import collections
    cnt = collections.Counter()
    x0 = ox + INSET + c*SPEC["cell_px"]
    y0 = oy + INSET + r*SPEC["cell_px"]
    for y in range(y0, y0 + SPEC["sprite_px"]):
        if y >= len(rows):
            break
        row = rows[y]
        for x in range(x0, x0 + SPEC["sprite_px"]):
            if x*3 + 2 >= len(row):
                break
            o = x*3
            rr, gg, bb = row[o], row[o+1], row[o+2]
            if max(rr, gg, bb) - min(rr, gg, bb) >= 60:
                cnt[(rr, gg, bb)] += 1
    if not cnt:
        return None
    # the most saturated frequent colour is the ball's base hue
    best, bn = None, 0
    for col, n in cnt.most_common(24):
        sat = max(col) - min(col)
        if sat > 70 and n * (1 + sat/255.0) > bn:
            best, bn = col, n
    return best or cnt.most_common(1)[0][0]


def name_colour(c):
    pal = SPEC["reference_palette"]
    best, bi = None, 1e9
    for i, p in enumerate(pal):
        d = abs(c[0]-p[0]) + abs(c[1]-p[1]) + abs(c[2]-p[2])
        if d < bi:
            best, bi = i, d
    return (PALETTE_NAMES[best], bi) if bi <= 150 else ("#%02X%02X%02X" % c, bi)


def extract_colours(rows, w, h, origin, side=9):
    ox, oy = origin
    board, palette = [], {}
    for r in range(side):
        line = []
        for c in range(side):
            col = slot_colour(rows, ox, oy, r, c)
            if col is None:
                line.append(0)
            else:
                nm, _ = name_colour(col)
                palette.setdefault(nm, col)
                line.append(nm)
        board.append(line)
    return board, palette


# ---------------------------------------------------------------- scores
def decode_score_table(blob):
    """Decode the 16 x 32 byte high-score table."""
    rec_len = SPEC["score_record_len"]
    name_len = SPEC["score_name_len"]
    off = SPEC["score_offset"]
    out = []
    for i in range(SPEC["score_entries"]):
        base = i*rec_len
        if base + rec_len > len(blob):
            break
        raw = blob[base:base + name_len]
        nm = raw.split(b"\x00")[0].decode("latin1", "replace")
        score = int.from_bytes(blob[base+off:base+off+4], "little")
        out.append({"index": i, "name": nm, "score": score,
                    "raw_name_hex": raw.hex()})
    return out


def decode_hash(blob):
    if len(blob) < 16:
        return []
    return [int.from_bytes(blob[i*4:(i+1)*4], "little") for i in range(4)]


# ---------------------------------------------------------------- reporting
class Report:
    def __init__(self):
        self.rows = []

    def add(self, name, expected, measured, note=""):
        ok = (str(expected) == str(measured))
        self.rows.append({"name": name, "expected": expected,
                           "measured": measured, "ok": ok, "note": note})
        return ok

    def add_bool(self, name, ok, expected="true", measured="", note=""):
        self.rows.append({"name": name, "expected": expected,
                           "measured": measured, "ok": bool(ok), "note": note})
        return ok

    def render(self):
        L = ["", "  %-34s %-22s %-22s %s" % ("ITEM", "EXPECTED", "MEASURED", "RESULT")]
        L.append("  " + "-"*94)
        for r in self.rows:
            L.append("  %-34s %-22s %-22s %s" % (
                r["name"], str(r["expected"])[:22], str(r["measured"])[:22],
                "OK" if r["ok"] else ("DIFF" + (" (" + r["note"] + ")" if r["note"] else ""))))
        bad = [r for r in self.rows if not r["ok"]]
        L.append("  " + "-"*94)
        L.append("  %d checks, %d match, %d differ" %
                 (len(self.rows), len(self.rows)-len(bad), len(bad)))
        return "\n".join(L)

    def to_json(self, path):
        os.makedirs(os.path.dirname(os.path.abspath(path)) or ".", exist_ok=True)
        json.dump({"checks": self.rows}, open(path, "w"), indent=1, ensure_ascii=False)


# ---------------------------------------------------------------- shot
def cmd_shot(a):
    w, h, rows = pngio.read_png(a.png)
    m = geometry.measure(rows, w, h, SPEC["board_side"])
    rep = Report()
    print("=== screenshot conformance: %s ===" % os.path.basename(a.png))
    if "error" in m:
        print("  ERROR: %s" % m["error"])
        return 1

    ox, oy = m["board_origin"]
    # the client area of the screenshot is assumed to be the whole image
    px, py, ppx = spec_layout(w, h)
    print("  measured board origin : (%d, %d)" % (ox, oy))
    print("  predicted board origin: (%d, %d)   [client %dx%d]" % (px, py, w, h))
    rep.add("board origin X", px, ox, "delta %+d" % (ox - px))
    rep.add("board origin Y", py, oy, "delta %+d" % (oy - py))
    rep.add("cell size px", SPEC["cell_px"], m["cell_px"])
    rep.add("grid side", SPEC["board_side"], m["grid_side"])
    rep.add("board frame width", SPEC["board_frame_w"],
            m["frame_rect"][2] - m["frame_rect"][0] + 1)
    rep.add("board frame height", SPEC["board_frame_h"],
            m["frame_rect"][3] - m["frame_rect"][1] + 1)
    rep.add_bool("grid coverage (all sprites inside grid)", m["coverage"] >= 0.95,
                 expected=">=0.95", measured=round(m["coverage"], 4))

    # ---- colour extraction (BB-N02) ----------------------------------
    board, palette = extract_colours(rows, w, h, (ox, oy), SPEC["board_side"])
    used = sorted({v for line in board for v in line if v != 0})
    print()
    print("  measured board map (colour name per slot):")
    for line in board:
        print("    " + " ".join("%-8s" % v for v in line))
    print("  distinct colours observed: %d -> %s" % (len(used), ", ".join(used)))
    for nm, rgb in sorted(palette.items()):
        print("    %-10s #%02X%02X%02X" % (nm, rgb[0], rgb[1], rgb[2]))
    rep.add("distinct colours observed", SPEC["colors"], len(used))
    rep.add_bool("colour names all known", all(u in PALETTE_NAMES for u in used),
                 expected="known names", measured=",".join(used))

    print(rep.render())

    if a.out:
        os.makedirs(a.out, exist_ok=True)
        rep.to_json(os.path.join(a.out, "conformance_shot.json"))
        json.dump(m, open(os.path.join(a.out, "geometry.json"), "w"), indent=1)
        json.dump({"board": board, "palette": {k: list(v) for k, v in palette.items()}},
                  open(os.path.join(a.out, "board_map.json"), "w"), indent=1)
        print("  -> %s" % a.out)
    return 0


# ---------------------------------------------------------------- probe
def cmd_probe(a):
    d = a.dir
    rep = Report()
    print("=== winprobe conformance: %s ===" % d)

    # ---- window tree
    wp = os.path.join(d, "windows.json")
    if os.path.exists(wp):
        wj = json.load(open(wp))
        wins = wj.get("windows", [])
        classes = [x["class"] for x in wins]
        rep.add("main window class present", True, SPEC["window_class"] in classes)
        cw = wj.get("target", {}).get("client", [0, 0, 0, 0])
        tw = cw[2] - cw[0]; th = cw[3] - cw[1]
        if tw:
            px, py, ppx = spec_layout(tw, th)
            rep.add("predicted board origin X", px, None, "client %dx%d" % (tw, th))
        rep.add("window count observed", ">0", len(wins))
        print("  windows: %d, classes: %s" %
              (len(wins), ", ".join(sorted(set(classes)))))
    else:
        print("  (no windows.json)")

    # ---- menu tree
    mp = os.path.join(d, "menu.json")
    if os.path.exists(mp):
        mj = json.load(open(mp))
        ids = [m["id"] for m in mj.get("menu", []) if m.get("id")]
        missing = [i for i in SPEC["menu_ids"] if i not in ids]
        rep.add("all expected menu ids present", [], missing)
        print("  runtime menu ids: %s" % sorted(set(ids)))
        acc = mj.get("accel", [])
        print("  runtime accelerators: %s" %
              ", ".join("%d=%d" % (x["cmd"], x["key"]) for x in acc))
        rep.add("accelerator count", 6, len(acc))
    else:
        print("  (no menu.json)")

    # ---- config
    cp = os.path.join(d, "config.json")
    if os.path.exists(cp):
        cj = json.load(open(cp))
        ini = cj.get("ini", [])
        if ini and ini[0].get("present"):
            raw = bytes.fromhex(ini[0]["data"])
            text = raw.decode("latin1")
            found = [k for k in SPEC["ini_keys"] if k.lower() in text.lower()]
            rep.add("INI keys found", len(SPEC["ini_keys"]), len(found),
                    "missing: %s" % [k for k in SPEC["ini_keys"] if k not in found])
            print("  INI present, %d bytes, keys: %s" % (len(raw), ", ".join(found)))
        else:
            print("  INI absent")
        for r in cj.get("registry", []):
            if not r.get("present"):
                continue
            size = r.get("size", 0)
            blob = bytes.fromhex(r.get("data", ""))
            if r["value"] == "Table":
                rep.add("Table size", SPEC["score_table_len"], size)
                recs = decode_score_table(blob)
                print("  high-score table (%d records of %d B):" %
                      (len(recs), SPEC["score_record_len"]))
                print("    %-4s %-28s %s" % ("#", "NAME", "SCORE"))
                for rec in recs:
                    print("    %-4d %-28s %d" % (rec["index"], rec["name"], rec["score"]))
                named = [x for x in recs if x["name"]]
                rep.add_bool("records carry a name", len(named) > 0,
                             expected=">0", measured="%d of %d" % (len(named), len(recs)),
                             note="%d of %d" % (len(named), len(recs)))
                maxlen = max([len(x["name"]) for x in recs] or [0])
                rep.add_bool("longest name fits the 28-byte field",
                             maxlen <= SPEC["score_name_len"],
                             expected="<= %d" % SPEC["score_name_len"], measured=maxlen)
                if a.out:
                    os.makedirs(a.out, exist_ok=True)
                    json.dump({"records": recs},
                              open(os.path.join(a.out, "score_table.json"), "w"),
                              indent=1, ensure_ascii=False)
            elif r["value"] == "Hash":
                rep.add("Hash size", SPEC["hash_len"], size)
                words = decode_hash(blob)
                print("  hash words: %s" %
                      ", ".join("0x%08X" % x for x in words))
                if a.out:
                    os.makedirs(a.out, exist_ok=True)
                    json.dump({"words": words},
                              open(os.path.join(a.out, "score_hash.json"), "w"), indent=1)
            print("  registry %s: %d bytes type=%s" % (r["value"], size, r.get("type")))
    else:
        print("  (no config.json)")

    print(rep.render())
    if a.out:
        rep.to_json(os.path.join(a.out, "conformance_probe.json"))
    return 0


# ---------------------------------------------------------------- close
def cmd_close(a):
    script = os.path.join(HERE, "checklist.py")
    cmd = [sys.executable, script, "set", a.id, "--status", "done",
           "--evidence", a.evidence]
    if a.note:
        cmd += ["--note", a.note]
    return subprocess.call(cmd)


def main():
    ap = argparse.ArgumentParser()
    sub = ap.add_subparsers(dest="cmd", required=True)
    p = sub.add_parser("shot"); p.add_argument("png"); p.add_argument("--out"); p.set_defaults(fn=cmd_shot)
    p = sub.add_parser("probe"); p.add_argument("dir"); p.add_argument("--out"); p.set_defaults(fn=cmd_probe)
    p = sub.add_parser("close"); p.add_argument("id"); p.add_argument("evidence"); p.add_argument("--note"); p.set_defaults(fn=cmd_close)
    a = ap.parse_args()
    sys.exit(a.fn(a) or 0)


if __name__ == "__main__":
    main()
