#!/usr/bin/env python3
"""Drive and capture a live WinLinez window.

This is the *input* half of the black-box tooling: winprobe only reads, this
script also sends window messages.  It deliberately posts messages to the
target window instead of moving the physical cursor, so experiments do not
steal focus from whatever else is running on the desktop.

Usage (all coordinates are client-relative; row/col are board cells):

    bbdrive.py --pid 1234 shot --out shot.png
    bbdrive.py --pid 1234 board
    bbdrive.py --pid 1234 click --row 2 --col 4
    bbdrive.py --pid 1234 move --from 2,4 --to 4,4
    bbdrive.py --pid 1234 burst --out frames --seconds 1.2 --interval 0.03
    bbdrive.py --pid 1234 cmd 257

The window class defaults to ``WinLinezWindowClass``.
"""
from __future__ import annotations

import argparse
import ctypes
import ctypes.wintypes as wt
import os
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import pngio  # noqa: E402

user32 = ctypes.WinDLL("user32", use_last_error=True)
gdi32 = ctypes.WinDLL("gdi32", use_last_error=True)

WM_MOUSEMOVE = 0x0200
WM_LBUTTONDOWN = 0x0201
WM_LBUTTONUP = 0x0202
WM_COMMAND = 0x0111
MK_LBUTTON = 0x0001
PW_CLIENTONLY = 1
PW_RENDERFULLCONTENT = 2


class RECT(ctypes.Structure):
    _fields_ = [("left", wt.LONG), ("top", wt.LONG),
                ("right", wt.LONG), ("bottom", wt.LONG)]


class BITMAPINFOHEADER(ctypes.Structure):
    _fields_ = [
        ("biSize", wt.DWORD), ("biWidth", wt.LONG), ("biHeight", wt.LONG),
        ("biPlanes", wt.WORD), ("biBitCount", wt.WORD),
        ("biCompression", wt.DWORD), ("biSizeImage", wt.DWORD),
        ("biXPelsPerMeter", wt.LONG), ("biYPelsPerMeter", wt.LONG),
        ("biClrUsed", wt.DWORD), ("biClrImportant", wt.DWORD),
    ]


class BITMAPINFO(ctypes.Structure):
    _fields_ = [("bmiHeader", BITMAPINFOHEADER),
                ("bmiColors", wt.DWORD * 3)]


user32.EnumWindows.argtypes = [ctypes.WINFUNCTYPE(wt.BOOL, wt.HWND, wt.LPARAM),
                               wt.LPARAM]
user32.GetClassNameW.argtypes = [wt.HWND, wt.LPWSTR, ctypes.c_int]
user32.GetWindowThreadProcessId.argtypes = [wt.HWND,
                                            ctypes.POINTER(wt.DWORD)]
user32.GetClientRect.argtypes = [wt.HWND, ctypes.POINTER(RECT)]
user32.GetWindowRect.argtypes = [wt.HWND, ctypes.POINTER(RECT)]
user32.PrintWindow.argtypes = [wt.HWND, wt.HDC, wt.UINT]
user32.PostMessageW.argtypes = [wt.HWND, wt.UINT, wt.WPARAM, wt.LPARAM]
gdi32.CreateCompatibleDC.argtypes = [wt.HDC]
gdi32.CreateDIBSection.argtypes = [wt.HDC, ctypes.POINTER(BITMAPINFO),
                                   wt.UINT, ctypes.POINTER(ctypes.c_void_p),
                                   wt.HANDLE, wt.DWORD]
gdi32.SelectObject.argtypes = [wt.HDC, wt.HGDIOBJ]
gdi32.DeleteObject.argtypes = [wt.HGDIOBJ]
gdi32.DeleteDC.argtypes = [wt.HDC]
gdi32.BitBlt.argtypes = [wt.HDC, ctypes.c_int, ctypes.c_int, ctypes.c_int,
                         ctypes.c_int, wt.HDC, ctypes.c_int, ctypes.c_int,
                         wt.DWORD]
SRCCOPY = 0x00CC0020


def find_window(pid=None, cls="WinLinezWindowClass"):
    found = []

    @ctypes.WINFUNCTYPE(wt.BOOL, wt.HWND, wt.LPARAM)
    def cb(hwnd, _lp):
        if pid:
            p = wt.DWORD()
            user32.GetWindowThreadProcessId(hwnd, ctypes.byref(p))
            if p.value != pid:
                return True
        buf = ctypes.create_unicode_buffer(256)
        user32.GetClassNameW(hwnd, buf, 256)
        if buf.value == cls:
            found.append(hwnd)
            return False
        return True

    user32.EnumWindows(cb, 0)
    if not found:
        raise SystemExit("no window of class %r%s" %
                         (cls, " for pid %s" % pid if pid else ""))
    return found[0]


def client_size(hwnd):
    r = RECT()
    user32.GetClientRect(hwnd, ctypes.byref(r))
    return r.right - r.left, r.bottom - r.top


def window_size(hwnd):
    r = RECT()
    user32.GetWindowRect(hwnd, ctypes.byref(r))
    return r.left, r.top, r.right - r.left, r.bottom - r.top


def capture(hwnd, client_only=True):
    """Return (w, h, rows) with rows as RGB bytearrays, like pngio."""
    cw, ch = client_size(hwnd)
    if client_only:
        w, h = cw, ch
    else:
        _x, _y, w, h = window_size(hwnd)

    hdc = user32.GetDC(hwnd)
    mem = gdi32.CreateCompatibleDC(hdc)
    bmi = BITMAPINFO()
    bmi.bmiHeader.biSize = ctypes.sizeof(BITMAPINFOHEADER)
    bmi.bmiHeader.biWidth = w
    bmi.bmiHeader.biHeight = -h          # top-down
    bmi.bmiHeader.biPlanes = 1
    bmi.bmiHeader.biBitCount = 32
    bmi.bmiHeader.biCompression = 0      # BI_RGB
    bits = ctypes.c_void_p()
    bmp = gdi32.CreateDIBSection(hdc, ctypes.byref(bmi), 0,
                                 ctypes.byref(bits), None, 0)
    old = gdi32.SelectObject(mem, bmp)
    if client_only:
        # BitBlt reads the window's own surface and, unlike PrintWindow,
        # does not need the target to process WM_PRINT -- essential for
        # frame-by-frame capture while the game is inside its Sleep(50)
        # refill loop.  It also works while the window is occluded.
        ok = gdi32.BitBlt(mem, 0, 0, w, h, hdc, 0, 0, SRCCOPY)
    else:
        ok = user32.PrintWindow(hwnd, mem, PW_RENDERFULLCONTENT)
    raw = ctypes.string_at(bits, w * h * 4)
    gdi32.SelectObject(mem, old)
    gdi32.DeleteObject(bmp)
    gdi32.DeleteDC(mem)
    user32.ReleaseDC(hwnd, hdc)
    if not ok:
        raise SystemExit("PrintWindow failed (flag=%d)" %
                         (PW_CLIENTONLY if client_only else
                          PW_RENDERFULLCONTENT))

    rows = []
    for y in range(h):
        src = raw[y * w * 4:(y + 1) * w * 4]
        row = bytearray(w * 3)
        row[0::3] = src[2::4]
        row[1::3] = src[1::4]
        row[2::3] = src[0::4]
        rows.append(row)
    return w, h, rows


def save_png(hwnd, path, client_only=True):
    w, h, rows = capture(hwnd, client_only)
    pngio.write_png(path, w, h, rows)
    return w, h


def _lparam(x, y):
    return (y << 16) | (x & 0xFFFF)


def move(hwnd, x, y, buttons=0):
    user32.PostMessageW(hwnd, WM_MOUSEMOVE, buttons, _lparam(x, y))


def click_xy(hwnd, x, y, settle=0.05):
    move(hwnd, x, y)
    user32.PostMessageW(hwnd, WM_LBUTTONDOWN, MK_LBUTTON, _lparam(x, y))
    time.sleep(settle)
    user32.PostMessageW(hwnd, WM_LBUTTONUP, 0, _lparam(x, y))
    time.sleep(0.15)


def drag_xy(hwnd, x0, y0, x1, y1, steps=8, settle=0.03):
    move(hwnd, x0, y0)
    user32.PostMessageW(hwnd, WM_LBUTTONDOWN, MK_LBUTTON, _lparam(x0, y0))
    time.sleep(settle)
    for i in range(1, steps + 1):
        x = x0 + (x1 - x0) * i // steps
        y = y0 + (y1 - y0) * i // steps
        user32.PostMessageW(hwnd, WM_MOUSEMOVE, MK_LBUTTON, _lparam(x, y))
        time.sleep(settle / 2)
    user32.PostMessageW(hwnd, WM_LBUTTONUP, 0, _lparam(x1, y1))
    time.sleep(0.2)


def send_command(hwnd, cmd):
    user32.PostMessageW(hwnd, WM_COMMAND, cmd & 0xFFFF, 0)
    time.sleep(0.2)


# ----------------------------------------------------------------- geometry
def _cdiv2(v):
    """C integer division by two (truncation toward zero)."""
    return -((-v) // 2) if v < 0 else v // 2


def layout(w, h):
    """Client geometry, transcribed from the original's WM_SIZE handler.

    board_x = (W - 328) / 2, board_y = (H - 376) / 2 + 46.  A live 604x371
    client gives (138, 44), which is what the screenshots show.
    """
    return _cdiv2(w - 328), _cdiv2(h - 376) + 46


def cell_center(w, h, row, col):
    bx, by = layout(w, h)
    return bx + 2 + 18 + col * 36, by + 2 + 18 + row * 36


def cell_at(w, h, x, y):
    bx, by = layout(w, h)
    dx, dy = x - (bx + 2), y - (by + 2)
    if dx < 0 or dy < 0:
        return None
    c, r = dx // 36, dy // 36
    if c >= 9 or r >= 9:
        return None
    return int(r), int(c)


def pixel(rows, x, y):
    o = x * 3
    return rows[y][o], rows[y][o + 1], rows[y][o + 2]


def board(rows, w, h):
    """Sample the 9x9 cell centres; return (r, c, (r,g,b))."""
    out = []
    for r in range(9):
        line = []
        for c in range(9):
            x, y = cell_center(w, h, r, c)
            line.append(pixel(rows, x, y))
        out.append(line)
    return out


def main(argv=None):
    ap = argparse.ArgumentParser()
    ap.add_argument("--pid", type=int)
    ap.add_argument("--class", dest="cls", default="WinLinezWindowClass")
    sub = ap.add_subparsers(dest="cmd", required=True)

    p = sub.add_parser("shot")
    p.add_argument("--out", required=True)
    p.add_argument("--full", action="store_true",
                   help="capture the whole window, not just the client area")
    p = sub.add_parser("board")
    p = sub.add_parser("click")
    p.add_argument("--row", type=int, required=True)
    p.add_argument("--col", type=int, required=True)
    p = sub.add_parser("move")
    p.add_argument("--from", dest="src", required=True)
    p.add_argument("--to", dest="dst", required=True)
    p = sub.add_parser("drag")
    p.add_argument("--from", dest="src", required=True)
    p.add_argument("--to", dest="dst", required=True)
    p = sub.add_parser("burst")
    p.add_argument("--out", required=True)
    p.add_argument("--seconds", type=float, default=1.0)
    p.add_argument("--interval", type=float, default=0.03)
    p = sub.add_parser("refill")
    p.add_argument("--from", dest="src", required=True)
    p.add_argument("--to", dest="dst", required=True)
    p.add_argument("--out", required=True)
    p.add_argument("--seconds", type=float, default=1.2)
    p.add_argument("--interval", type=float, default=0.015)
    p = sub.add_parser("cmd")
    p.add_argument("id", type=int)

    a = ap.parse_args(argv)
    hwnd = find_window(a.pid, a.cls)
    w, h = client_size(hwnd)

    def rc(spec):
        r, c = [int(v) for v in spec.split(",")]
        return cell_center(w, h, r, c)

    if a.cmd == "shot":
        n = save_png(hwnd, a.out, client_only=not a.full)
        print("wrote %s (%dx%d)" % (a.out, n[0], n[1]))
    elif a.cmd == "board":
        _w, _h, rows = capture(hwnd)
        for r, line in enumerate(board(rows, w, h)):
            print(r, " ".join("%02x%02x%02x" % p for p in line))
    elif a.cmd == "click":
        click_xy(hwnd, *cell_center(w, h, a.row, a.col))
    elif a.cmd == "move":
        x0, y0 = rc(a.src)
        x1, y1 = rc(a.dst)
        click_xy(hwnd, x0, y0)
        click_xy(hwnd, x1, y1)
    elif a.cmd == "drag":
        x0, y0 = rc(a.src)
        x1, y1 = rc(a.dst)
        drag_xy(hwnd, x0, y0, x1, y1)
    elif a.cmd == "burst":
        os.makedirs(a.out, exist_ok=True)
        t0 = time.time()
        i = 0
        while time.time() - t0 < a.seconds:
            w2, h2, rows = capture(hwnd)
            pngio.write_png(os.path.join(a.out, "f%03d.png" % i), w2, h2, rows)
            i += 1
            time.sleep(a.interval)
        print("wrote %d frames to %s" % (i, a.out))
    elif a.cmd == "refill":
        os.makedirs(a.out, exist_ok=True)
        x0, y0 = rc(a.src)
        x1, y1 = rc(a.dst)
        click_xy(hwnd, x0, y0)              # select the ball first
        move(hwnd, x1, y1)
        user32.PostMessageW(hwnd, WM_LBUTTONDOWN, MK_LBUTTON,
                            _lparam(x1, y1))
        user32.PostMessageW(hwnd, WM_LBUTTONUP, 0, _lparam(x1, y1))
        t0 = time.time()
        i = 0
        while time.time() - t0 < a.seconds:
            w2, h2, rows = capture(hwnd)
            pngio.write_png(os.path.join(a.out, "f%03d.png" % i), w2, h2, rows)
            i += 1
            time.sleep(a.interval)
        print("wrote %d frames to %s" % (i, a.out))
    elif a.cmd == "cmd":
        send_command(hwnd, a.id)
    return 0


if __name__ == "__main__":
    sys.exit(main())
