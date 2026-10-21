#!/usr/bin/env python3
"""
Measure the board geometry from a screenshot of the running game.

Answers the black-box items:
    BB-U02  client-area geometry of every region
    BB-U04  pixel position of the board grid
    BB-N01  board side length (cells per row)

Method (it measures, it does not assume the spec formulas):
  1. FRAME  find the sunken 3D frame that surrounds the board.  Its long
            horizontal border runs are grouped into rectangle pairs; the
            rectangle's top-left corner is the board origin.
  2. GRID   mask the strongly coloured pixels (the sprites are the only
            saturated objects on the Win95-grey board), assign them to the
            9x9 slots of the candidate origin, and score the fit.
  3. PAD    the residual offset between the frame and the first coloured pixel
            is reported: it is the sprite art's internal padding, which is a
            useful calibration number for the art pipeline.

Usage:
    python3 tools/blackbox/geometry.py screenshot.png [--json out.json]
"""
import argparse, json, os, sys
from collections import Counter

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import pngio

CELL, SPRITE, INSET = 36, 32, 2
SAT_MIN = 60          # minimum channel spread for a pixel to count as coloured
BORDER_LO, BORDER_HI = 0x20, 0xB0   # grayscale range of the sunken frame lines
MIN_RUN = 200         # minimum length of a frame run


def analyse_pixels(rows, w, h):
    """Return (saturated points, border runs)."""
    pts, hruns, vruns = [], [], []
    for y in range(h):
        r = rows[y]
        run_start, run_len = -1, 0
        for x in range(w):
            o = x*3
            rr, gg, bb = r[o], r[o+1], r[o+2]
            if max(rr, gg, bb) - min(rr, gg, bb) >= SAT_MIN:
                pts.append((x, y))
            if rr == gg == bb and BORDER_LO <= rr <= BORDER_HI:
                if run_len == 0:
                    run_start = x
                run_len += 1
            else:
                if run_len >= MIN_RUN:
                    hruns.append((run_start, x-1, y))
                run_len = 0
        if run_len >= MIN_RUN:
            hruns.append((run_start, w-1, y))
    return pts, hruns


def find_frame_rects(hruns):
    """Group horizontal runs into rectangles (top/bottom edges)."""
    rects = []
    by_span = {}
    for x0, x1, y in hruns:
        by_span.setdefault((x0, x1), []).append(y)
    for (x0, x1), ys in by_span.items():
        ys = sorted(set(ys))
        for i in range(len(ys)):
            for j in range(i+1, len(ys)):
                h = ys[j] - ys[i]
                if 200 <= h <= 500:
                    rects.append((x0, ys[i], x1, ys[j], x1-x0+1, h))
    return rects


def score_origin(pts, ox, oy, side=9):
    occupied = set()
    inside = 0
    for x, y in pts:
        c = (x - ox - INSET)//CELL
        r = (y - oy - INSET)//CELL
        if 0 <= c < side and 0 <= r < side:
            inside += 1
            occupied.add((r, c))
    return len(occupied), inside


def measure(rows, w, h, side=9):
    out = {"image": [w, h], "cell_px": CELL, "sprite_px": SPRITE, "inset": INSET}
    pts, hruns = analyse_pixels(rows, w, h)
    out["coloured_pixels"] = len(pts)
    if not pts:
        out["error"] = "no coloured pixels found -- is this the game window?"
        return out

    rects = find_frame_rects(hruns)
    out["frame_candidates"] = len(rects)

    best = None
    for x0, y0, x1, y1, rw, rh in rects:
        for ox, oy in ((x0, y0), (x0, y1 - rh + 1), (x0, y0)):
            occ, inside = score_origin(pts, ox, oy, side)
            if inside < len(pts)*0.95:
                continue
            if best is None or occ > best[0]:
                best = (occ, inside, ox, oy, (x0, y0, x1, y1))
    if best is None:
        out["error"] = ("no frame rectangle found that fits a %dx%d grid "
                        "(found %d frame candidates)" % (side, side, len(rects)))
        return out

    occ, inside, ox, oy, rect = best
    out["board_origin"] = [ox, oy]
    out["frame_rect"] = list(rect)
    out["occupied_cells"] = occ
    out["grid_side"] = side
    out["coverage"] = round(inside/max(1, len(pts)), 4)

    cmin_x = min(p[0] for p in pts)
    cmin_y = min(p[1] for p in pts)
    out["first_coloured_px"] = [cmin_x, cmin_y]
    out["sprite_pad"] = [cmin_x - (ox + INSET), cmin_y - (oy + INSET)]
    out["board_px"] = [side*CELL, side*CELL]
    out["empty_slots"] = side*side - occ
    return out


def report(m):
    if "error" in m:
        return "  ERROR: %s" % m["error"]
    ox, oy = m["board_origin"]
    L = []
    L.append("  image size        : %dx%d" % (m["image"][0], m["image"][1]))
    L.append("  coloured pixels   : %d" % m["coloured_pixels"])
    L.append("  frame rect        : %s" % (m["frame_rect"],))
    L.append("  board origin      : (%d, %d)" % (ox, oy))
    L.append("  grid              : %dx%d cells of %d px" %
             (m["grid_side"], m["grid_side"], m["cell_px"]))
    L.append("  occupied slots    : %d of %d  (%d empty)" %
             (m["occupied_cells"], m["grid_side"]**2, m["empty_slots"]))
    L.append("  sprite art padding: %s px from the cell edge" % (m["sprite_pad"],))
    L.append("  grid coverage     : %.4f" % m["coverage"])
    return "\n".join(L)


if __name__ == "__main__":
    ap = argparse.ArgumentParser()
    ap.add_argument("image")
    ap.add_argument("--json")
    ap.add_argument("--side", type=int, default=9)
    a = ap.parse_args()

    w, h, rows = pngio.read_png(a.image)
    m = measure(rows, w, h, a.side)
    print("=== geometry of %s ===" % os.path.basename(a.image))
    print(report(m))
    if a.json:
        json.dump(m, open(a.json, "w"), indent=1)
        print("  -> %s" % a.json)
    sys.exit(1 if "error" in m else 0)
