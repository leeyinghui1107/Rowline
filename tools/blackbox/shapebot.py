#!/usr/bin/env python3
"""Fill an arbitrary set of board cells with one colour, then watch it.

This is the companion to ``linebot.py`` for shapes that are not a straight
run: an L of five, a cross, a two-line bridge.  It uses the live window's
own move rule (a ball may travel through empty cells in its 4-connected
region) and never guesses whether a move succeeded -- it re-reads the board
after every click.

    shapebot.py --pid 1234 fill --color magenta \
        --cells "8,4 8,5 8,6 8,7 7,7" --shot C:\\bb\\shots\\lshape.png

Exit codes: 0 = shape filled (stable or cleared), 1 = gave up.
"""
from __future__ import annotations

import argparse
import os
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import bbdrive  # noqa: E402
import linebot   # noqa: E402

EMPTY = None


def parse_cells(text):
    out = []
    for part in text.split():
        r, c = part.split(",")
        out.append((int(r), int(c)))
    return out


def read(hwnd):
    return linebot.read(hwnd)


def move(hwnd, w, h, src, dst, settle):
    linebot.move(hwnd, w, h, src, dst, settle=settle)


def blocker_target(grid, blocker, shape):
    """An empty cell outside the shape that the blocker can reach."""
    empties = [(r, c) for r in range(9) for c in range(9)
               if not grid[r][c] and (r, c) not in shape]
    # Prefer a far corner so the blocker does not land next to the shape.
    empties.sort(key=lambda p: -(abs(p[0] - blocker[0]) + abs(p[1] - blocker[1])))
    for dst in empties:
        if linebot.reachable(grid, blocker, dst):
            return dst
    return None


def fill(hwnd, color, shape, settle, shot):
    for step in range(40):
        w, h, _rows, grid, preview = read(hwnd)
        have = [p for p in shape if grid[p[0]][p[1]] == color]
        wrong = [p for p in shape if grid[p[0]][p[1]] not in (None, color)]
        print("step %d: have %d/%d, wrong %s, preview %s"
              % (step, len(have), len(shape), wrong, preview))
        if len(have) == len(shape):
            print("shape filled; waiting 2 s to see whether it clears")
            time.sleep(2.0)
            _w, _h, _r, grid2, _p = read(hwnd)
            still = all(grid2[p[0]][p[1]] == color for p in shape)
            print("still there:", still)
            if shot:
                bbdrive.save_png(hwnd, shot)
                print("wrote", shot)
            return 0
        if wrong:
            # Move a blocker out of the shape (this consumes a turn, and the
            # refill may put something else into the shape; the loop retries).
            blocker = wrong[0]
            dst = blocker_target(grid, blocker, set(shape))
            if not dst:
                print("blocker", blocker, "has nowhere to go")
                return 1
            print("clear blocker", blocker, "->", dst)
            move(hwnd, w, h, blocker, dst, settle)
            continue
        # Find a reachable source of the right colour outside the shape and
        # an empty target inside it.  Prefer sources closest to the target.
        empty_targets = [p for p in shape if not grid[p[0]][p[1]]]
        choice = None
        for dst in empty_targets:
            sources = [(r, c) for r in range(9) for c in range(9)
                       if grid[r][c] == color and (r, c) not in shape]
            sources.sort(key=lambda p: abs(p[0] - dst[0]) + abs(p[1] - dst[1]))
            for src in sources:
                if linebot.reachable(grid, src, dst):
                    choice = (src, dst)
                    break
            if choice:
                break
        if not choice:
            print("no reachable source for", color)
            return 1
        src, dst = choice
        print("move", src, "->", dst)
        move(hwnd, w, h, src, dst, settle)
        _w, _h, _r, grid2, prev2 = read(hwnd)
        print("  now %s, preview %s"
              % ([p for p in shape if grid2[p[0]][p[1]] == color], prev2))
    print("gave up after 40 steps")
    return 1


def main(argv=None):
    ap = argparse.ArgumentParser()
    ap.add_argument("--pid", type=int, required=True)
    sub = ap.add_subparsers(dest="cmd", required=True)
    p = sub.add_parser("fill")
    p.add_argument("--color", required=True)
    p.add_argument("--cells", required=True)
    p.add_argument("--settle", type=float, default=0.45)
    p.add_argument("--shot")
    a = ap.parse_args(argv)
    if a.cmd != "fill":
        return 2
    shape = parse_cells(a.cells)
    hwnd = bbdrive.find_window(a.pid)
    return fill(hwnd, a.color, shape, a.settle, a.shot)


if __name__ == "__main__":
    sys.exit(main())
