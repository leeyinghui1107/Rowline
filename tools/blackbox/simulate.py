#!/usr/bin/env python3
"""
Render a *mock* of the original game window.

Purpose: exercise the black-box tooling (geometry measurement, conformance
reporting) without needing the original executable, and provide a regression
fixture for the layout formulas.

It uses the original sprite sheet only as a rendering fixture; the produced
image is a test artefact, not a product asset.

Usage:
    python3 tools/blackbox/simulate.py --out /tmp/mock.png --size 1024x768 --seed 7
"""
import argparse, os, random, sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
ROOT = os.path.abspath(os.path.join(HERE, "..", ".."))
sys.path.insert(0, os.path.join(ROOT, "tools"))
import pngio

CELL, SPRITE, INSET = 36, 32, 2
SRC_Y = 225
SIDE = 9
BG = (192, 192, 192)


def idiv2(v):
    """C integer division by two (truncation toward zero)."""
    return -((-v) // 2) if v < 0 else v // 2


def layout(client_w, client_h):
    """The confirmed board origin (docs/11 section 5d).

    X = (W - 0x148) / 2, Y = (H - 0x178) / 2 + 0x2E, and the right figure
    sits at t + 0x148 + (t - 0x3A) / 2.  A live 604x371 client gives
    (138, 44).
    """
    t = idiv2(client_w - 328)
    panel_x = t + 328 + idiv2(t - 58)
    board_y = idiv2(client_h - 376) + 46
    return t, board_y, panel_x


def load_sheet(path):
    w, h, rows = pngio.read_png(path)
    return w, h, rows


def blit(dst, dw, sx, sy, sw, sh, dx, dy):
    dh = len(dst)
    for y in range(sh):
        if dy + y < 0 or dy + y >= dh:
            continue
        srow = rows_src[sy + y]
        drow = dst[dy + y]
        for x in range(sw):
            px = dx + x
            if px < 0 or px*3 + 3 > len(drow):
                continue
            o = (sx + x)*3
            drow[px*3:px*3 + 3] = srow[o:o+3]


def render(sheet_path, client_w, client_h, seed, set_mode=0, fill=0.75):
    global rows_src
    sw, sh, rows_src = load_sheet(sheet_path)

    img = [bytearray(bytes(BG)*client_w) for _ in range(client_h)]

    rng = random.Random(seed)
    board = []
    for i in range(SIDE*SIDE):
        board.append(rng.randint(1, 7) if rng.random() < fill else 0)

    bx, by, px = layout(client_w, client_h)
    # board frame (simple sunken look): 327 wide by 328 tall
    for y in range(by, min(client_h, by + 328)):
        for x in range(bx, min(client_w, bx + 327)):
            edge = (x in (bx, bx+326)) or (y in (by, by+327))
            v = (128, 128, 128) if edge else BG
            img[y][x*3:x*3+3] = bytes(v)
    # side panel
    for y in range(by, min(client_h, by + 328)):
        for x in range(px, min(client_w, px + 125)):
            img[y][x*3:x*3+3] = bytes(BG)
    # cells
    for r in range(SIDE):
        for c in range(SIDE):
            v = board[r*SIDE + c]
            if v == 0:
                continue
            srcx = v*CELL + set_mode*288
            blit(img, client_w, srcx, SRC_Y, SPRITE, SPRITE,
                 bx + INSET + c*CELL, by + INSET + r*CELL)
    return img, board, (bx, by, px)


def write_mock_config(path, seed):
    """
    Write a mock winprobe-style config.json (INI + registry).

    The registry blob uses the exact layout recovered from the original:
    16 records x 32 bytes, NUL-padded 28-byte name, u32 score at +0x1C,
    followed by a 16-byte "Hash" of four u32 words.
    """
    import json, struct
    rec_len, name_len, score_off = 32, 28, 0x1C
    table = bytearray(16 * rec_len)
    names = ["PLAYER", "GOLUBEV", "WINLINEZ", "AAA", "BBB", "CCC", "DDD",
             "EEE", "FFF", "GGG", "HHH", "III", "JJJ", "KKK", "LLL", "MMM"]
    scores = [100, 950, 800, 750, 640, 520, 430, 350, 280, 210, 170,
              130, 90, 60, 30, 0]
    for i in range(16):
        raw = names[i].encode("latin1")[:name_len]
        table[i*rec_len:i*rec_len+len(raw)] = raw
        struct.pack_into("<I", table, i*rec_len + score_off, scores[i])
    hash_words = [0x11223344, 0x55667788, 0x99AABBCC, 0xDDEEFF00]
    hblob = b"".join(struct.pack("<I", w) for w in hash_words)

    ini = ("[Settings]\r\n"
           "WindowX=120\r\nWindowY=80\r\nWindowWidth=1280\r\nWindowHeight=960\r\n"
           "WindowMaximized=0\r\nShowNext=1\r\nShowStatistics=1\r\nUseFigures=0\r\n")
    doc = {
        "ini": [{"path": "C:\\WinLinez.INI", "present": True,
                 "size": len(ini), "data": ini.encode("latin1").hex()}],
        "registry": [
            {"key": "SOFTWARE\\Ivan Golubev\\WinLinez\\HighScore", "value": "Table",
             "present": True, "type": 3, "size": len(table), "data": table.hex()},
            {"key": "SOFTWARE\\Ivan Golubev\\WinLinez\\HighScore", "value": "Hash",
             "present": True, "type": 3, "size": len(hblob), "data": hblob.hex()},
        ],
    }
    os.makedirs(os.path.dirname(os.path.abspath(path)) or ".", exist_ok=True)
    json.dump(doc, open(path, "w", encoding="utf-8"), indent=1)


if __name__ == "__main__":
    ap = argparse.ArgumentParser()
    ap.add_argument("--sheet", default=os.path.join(ROOT, "assets", "png", "BITMAP_1111_0.png"))
    ap.add_argument("--out", required=True)
    ap.add_argument("--size", default="1024x768")
    ap.add_argument("--seed", type=int, default=7)
    ap.add_argument("--set", type=int, default=0, choices=(0, 1))
    ap.add_argument("--config", help="also write a mock winprobe config.json here")
    a = ap.parse_args()

    cw, ch = (int(v) for v in a.size.lower().split("x"))
    img, board, geo = render(a.sheet, cw, ch, a.seed, a.set)
    pngio.write_png(a.out, cw, ch, img)
    bx, by, px = geo
    print("wrote %s (%dx%d)" % (a.out, cw, ch))
    print("  expected board origin : (%d, %d)" % (bx, by))
    print("  expected panel origin : (%d, %d)" % (px, by))
    print("  board seed             : %s" % " ".join(str(v) for v in board))

    if a.config:
        write_mock_config(a.config, a.seed)
        print("  wrote mock config      : %s" % a.config)
