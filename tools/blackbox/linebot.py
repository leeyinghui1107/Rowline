#!/usr/bin/env python3
"""A small black-box experiment bot for the live WinLinez window.

It reads the board from the window surface (BitBlt), then posts mouse messages
to move balls.  It is used to reach states that are hard to reach by hand:
a four-in-a-row that must NOT clear, the fifth ball that must clear, a six,
an idle board, and so on.

    linebot.py --pid 1234 show
    linebot.py --pid 1234 play --moves 6
    linebot.py --pid 1234 build --color green --need 4
    linebot.py --pid 1234 line --color green --length 4 --capture out

Every move is followed by a short settle interval (the original spawns the
next group of three with 50 ms between balls) and by a fresh board read.
"""
from __future__ import annotations

import argparse
import os
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import bbdrive  # noqa: E402
import pngio    # noqa: E402

EMPTY = (192, 192, 192)
COLORS = {
    "green": (0, 222, 0),
    "blue": (0, 0, 242),
    "cyan": (0, 242, 242),
    "magenta": (212, 0, 212),
    "red": (242, 0, 0),
    "yellow": (250, 239, 0),
    "maroon": (145, 0, 0),
}
PREVIEW_X = (265, 301, 337)
PREVIEW_Y = 22


def classify(rgb):
    if rgb == EMPTY:
        return None
    best = None
    bestd = 10 ** 9
    for name, ref in COLORS.items():
        d = sum((a - b) ** 2 for a, b in zip(rgb, ref))
        if d < bestd:
            best, bestd = name, d
    if bestd <= 70 ** 2:
        return best
    return "?%02x%02x%02x" % rgb


def read(hwnd):
    w, h, rows = bbdrive.capture(hwnd)
    grid = []
    for r in range(9):
        line = []
        for c in range(9):
            x, y = bbdrive.cell_center(w, h, r, c)
            line.append(classify(bbdrive.pixel(rows, x, y)))
        grid.append(line)
    preview = [classify(bbdrive.pixel(rows, x, PREVIEW_Y)) for x in PREVIEW_X]
    return w, h, rows, grid, preview


def show(grid, preview):
    for r, line in enumerate(grid):
        print("%d " % r + " ".join((v or ".")[:3].rjust(3) for v in line))
    print("preview:", preview)


def counts(grid):
    out = {}
    for line in grid:
        for v in line:
            if v:
                out[v] = out.get(v, 0) + 1
    return out


def move(hwnd, w, h, src, dst, settle=0.4):
    bbdrive.click_xy(hwnd, *bbdrive.cell_center(w, h, *src))
    bbdrive.click_xy(hwnd, *bbdrive.cell_center(w, h, *dst))
    time.sleep(settle)


def pick_move(grid, adjacent=False):
    """A harmless move: an existing ball to an empty cell."""
    balls = [(r, c) for r in range(9) for c in range(9) if grid[r][c]]
    empty = [(r, c) for r in range(9) for c in range(9) if not grid[r][c]]
    if not balls or not empty:
        return None
    if adjacent:
        for src in balls:
            for dr, dc in ((-1, 0), (1, 0), (0, -1), (0, 1)):
                dst = (src[0] + dr, src[1] + dc)
                if 0 <= dst[0] < 9 and 0 <= dst[1] < 9 and not grid[dst[0]][dst[1]]:
                    return src, dst
    # Prefer a ball in the top-left area and a destination in the bottom-right,
    # so we do not accidentally build a line while accumulating.
    src = min(balls, key=lambda p: (p[0] + p[1], p))
    dst = max(empty, key=lambda p: (p[0] + p[1], p))
    return src, dst


def reachable(grid, src, dst):
    """Can a ball at src move to dst through empty cells?

    The live window rejects moves whose destination is not in the same
    empty-space region as the source; this is the same 4-neighbour flood
    fill the original performs before every move.
    """
    from collections import deque
    seen = {src}
    q = deque([src])
    while q:
        r, c = q.popleft()
        if (r, c) == dst:
            return True
        for dr, dc in ((-1, 0), (1, 0), (0, -1), (0, 1)):
            n = (r + dr, c + dc)
            if not (0 <= n[0] < 9 and 0 <= n[1] < 9) or n in seen:
                continue
            # The source cell itself is passable; every other occupied
            # cell blocks the path.
            if n != src and grid[n[0]][n[1]] is not None:
                continue
            seen.add(n)
            q.append(n)
    return False


def line_candidates(grid, color, need):
    """All horizontal/vertical runs of `need` cells that are empty or color."""
    out = []
    for r in range(9):
        for c in range(9 - need + 1):
            cells = [(r, c + i) for i in range(need)]
            if all(grid[x][y] in (None, color) for x, y in cells):
                out.append(cells)
    for c in range(9):
        for r in range(9 - need + 1):
            cells = [(r + i, c) for i in range(need)]
            if all(grid[x][y] in (None, color) for x, y in cells):
                out.append(cells)
    return out


def build_line(hwnd, args):
    color = args.color
    need = args.length
    rejected = set()
    for step in range(40):
        w, h, _rows, grid, preview = read(hwnd)
        if args.start:
            r0, c0 = [int(v) for v in args.start.split(",")]
            if args.orient == "v":
                cells = [(r0 + i, c0) for i in range(need)]
            else:
                cells = [(r0, c0 + i) for i in range(need)]
            cands = [cells]
        else:
            cands = line_candidates(grid, color, need)
        if not cands:
            print("no free line for", color)
            return 1
        # Prefer a line that already holds the most target balls and that
        # still has a legal (reachable) move available.
        cands.sort(key=lambda cells: -sum(grid[r][c] == color for r, c in cells))
        choice = None
        for cells in cands:
            if any(grid[r][c] not in (None, color) for r, c in cells):
                continue
            for dst in ((r, c) for r, c in cells if not grid[r][c]):
                for src in ((r, c) for r in range(9) for c in range(9)
                            if grid[r][c] == color and (r, c) not in cells):
                    if (src, dst) in rejected:
                        continue
                    if reachable(grid, src, dst):
                        choice = (cells, src, dst)
                        break
                if choice:
                    break
            if choice:
                break
        if not choice:
            print("no reachable move for", color)
            return 1
        cells, src, dst = choice
        have = sum(grid[r][c] == color for r, c in cells)
        if have >= args.stop_at:
            print("line", cells, "already has", have)
            return 0
        print("target line:", cells, "have", have)
        print("move", src, "->", dst)
        will_complete = have + 1 >= args.stop_at
        if args.burst and will_complete:
            os.makedirs(args.burst, exist_ok=True)
            bbdrive.click_xy(hwnd, *bbdrive.cell_center(w, h, *src))
            x, y = bbdrive.cell_center(w, h, *dst)
            bbdrive.move(hwnd, x, y)
            bbdrive.user32.PostMessageW(hwnd, bbdrive.WM_LBUTTONDOWN,
                                        bbdrive.MK_LBUTTON,
                                        bbdrive._lparam(x, y))
            bbdrive.user32.PostMessageW(hwnd, bbdrive.WM_LBUTTONUP, 0,
                                        bbdrive._lparam(x, y))
            t0 = time.time()
            i = 0
            while time.time() - t0 < args.burst_seconds:
                w2, h2, rows2 = bbdrive.capture(hwnd)
                pngio.write_png(os.path.join(args.burst, "f%03d.png" % i),
                                w2, h2, rows2)
                i += 1
                time.sleep(0.015)
            print("  burst: %d frames -> %s" % (i, args.burst))
        else:
            move(hwnd, w, h, src, dst, settle=args.settle)
        _w, _h, _rows, grid2, _prev = read(hwnd)
        # A spawn may refill the source cell with any colour, so the source\n        # being non-empty does not mean the move failed.  The destination,\n        # however, is occupied by the moved ball before the spawn loop and\n        # can never be overwritten by it.\n        if grid2[dst[0]][dst[1]] == color:\n            n = sum(grid2[r][c] == color for r, c in cells)\n            print("  line now has", n)\n            if n >= args.stop_at:\n                return 0\n        else:\n            print("  move rejected; trying another pair")\n            rejected.add((src, dst))
    return 0


def main(argv=None):
    ap = argparse.ArgumentParser()
    ap.add_argument("--pid", type=int, required=True)
    sub = ap.add_subparsers(dest="cmd", required=True)
    sub.add_parser("show")
    p = sub.add_parser("play")
    p.add_argument("--moves", type=int, default=1)
    p.add_argument("--settle", type=float, default=0.4)
    p.add_argument("--adjacent", action="store_true")
    p = sub.add_parser("build")
    p.add_argument("--color", required=True)
    p.add_argument("--length", type=int, default=5)
    p.add_argument("--stop-at", type=int, default=4)
    p.add_argument("--settle", type=float, default=0.4)
    p.add_argument("--start", help="force the run to start at R,C")
    p.add_argument("--orient", choices=["h", "v"], default="h")
    p.add_argument("--burst", help="capture frames after the completing move")
    p.add_argument("--burst-seconds", type=float, default=1.0)
    p = sub.add_parser("shot")
    p.add_argument("--out", required=True)
    a = ap.parse_args(argv)

    hwnd = bbdrive.find_window(a.pid)
    if a.cmd == "shot":
        bbdrive.save_png(hwnd, a.out)
        return 0
    if a.cmd == "show":
        w, h, _rows, grid, preview = read(hwnd)
        show(grid, preview)
        print("counts:", counts(grid))
        return 0
    if a.cmd == "build":
        return build_line(hwnd, a)
    if a.cmd == "play":
        for i in range(a.moves):
            w, h, _rows, grid, preview = read(hwnd)
            mv = pick_move(grid, adjacent=a.adjacent)
            if not mv:
                print("no move")
                return 1
            print("move %d: %s -> %s" % (i + 1, mv[0], mv[1]))
            move(hwnd, w, h, mv[0], mv[1], settle=a.settle)
            _w, _h, _r, grid2, prev2 = read(hwnd)
            print("  counts:", counts(grid2), "preview:", prev2)
        w, h, _rows, grid, preview = read(hwnd)
        show(grid, preview)
        return 0
    return 0


if __name__ == "__main__":
    sys.exit(main())
