#!/usr/bin/env python3
"""
Input recorder for the black-box phase (Windows).

Logs key presses and mouse events with timestamps so that every recorded
action can be replayed exactly.  Designed to run on the analysis VM while the
game is in the foreground.

    python3 tools/blackbox/inputlog.py --out capture.csv            # 60 s run
    python3 tools/blackbox/inputlog.py --out capture.csv --seconds 600

Press F8 to drop a MARKER row: use it to segment the capture into the
individual black-box cases.

Output columns:
    t_ms, kind, key, vk, x, y, extra
where kind is one of: KEYDOWN, KEYUP, MOUSEDOWN, MOUSEUP, MOUSEMOVE, MARKER
"""
import argparse, csv, ctypes, os, sys, time

if sys.platform != "win32":
    sys.stderr.write("inputlog: this recorder targets Windows (uses GetAsyncKeyState).\n"
                     "On other platforms use tools/blackbox/inputlog_ahk.ahk instead.\n")
    sys.exit(2)

user32 = ctypes.windll.user32

VK_NAMES = {
    0x08: "BACKSPACE", 0x09: "TAB", 0x0D: "ENTER", 0x1B: "ESC", 0x20: "SPACE",
    0x21: "PRIOR", 0x22: "NEXT", 0x23: "END", 0x24: "HOME", 0x25: "LEFT",
    0x26: "UP", 0x27: "RIGHT", 0x28: "DOWN", 0x2D: "INSERT", 0x2E: "DELETE",
    0x70: "F1", 0x71: "F2", 0x72: "F3", 0x73: "F4", 0x74: "F5", 0x75: "F6",
    0x76: "F7", 0x77: "F8", 0x78: "F9", 0x79: "F10", 0x7A: "F11", 0x7B: "F12",
    0xA0: "LSHIFT", 0xA1: "RSHIFT", 0xA2: "LCtrl", 0xA3: "RCtrl",
    0xA4: "LALT", 0xA5: "RALT", 0x5B: "LWIN",
    0x01: "LBUTTON", 0x02: "RBUTTON", 0x04: "MBUTTON",
}

MARK_VK = 0x77   # F8


def key_name(vk):
    if vk in VK_NAMES:
        return VK_NAMES[vk]
    if 0x30 <= vk <= 0x5A:
        return chr(vk)
    if 0x60 <= vk <= 0x69:
        return "NUMPAD%d" % (vk - 0x60)
    return "VK_%02X" % vk


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--out", required=True)
    ap.add_argument("--seconds", type=float, default=60.0)
    ap.add_argument("--hz", type=int, default=100)
    ap.add_argument("--no-mouse-move", action="store_true")
    a = ap.parse_args()

    os.makedirs(os.path.dirname(os.path.abspath(a.out)) or ".", exist_ok=True)
    prev = {}
    t0 = time.perf_counter()
    mouse_down = False

    with open(a.out, "w", encoding="utf-8", newline="") as fh:
        w = csv.writer(fh)
        w.writerow(["t_ms", "kind", "key", "vk", "x", "y", "extra"])
        while time.perf_counter() - t0 < a.seconds:
            now = (time.perf_counter() - t0) * 1000.0
            # ---- keyboard
            for vk in range(1, 256):
                st = user32.GetAsyncKeyState(vk)
                down = bool(st & 0x8000)
                was = prev.get(vk, False)
                if down != was:
                    if vk == MARK_VK:
                        w.writerow([round(now), "MARKER", "F8", vk, "", "", ""])
                    else:
                        w.writerow([round(now), "KEYDOWN" if down else "KEYUP",
                                    key_name(vk), vk, "", "", ""])
                    prev[vk] = down
            # ---- mouse
            pt = ctypes.wintypes.POINT() if hasattr(ctypes, "wintypes") else None
            class POINT(ctypes.Structure):
                _fields_ = [("x", ctypes.c_long), ("y", ctypes.c_long)]
            p = POINT()
            user32.GetCursorPos(ctypes.byref(p))
            lb = bool(user32.GetAsyncKeyState(0x01) & 0x8000)
            if lb and not mouse_down:
                w.writerow([round(now), "MOUSEDOWN", "LBUTTON", 1, p.x, p.y, ""])
            elif not lb and mouse_down:
                w.writerow([round(now), "MOUSEUP", "LBUTTON", 1, p.x, p.y, ""])
            mouse_down = lb
            if not a.no_mouse_move:
                w.writerow([round(now), "MOUSEMOVE", "", "", p.x, p.y, ""])
            fh.flush()
            time.sleep(1.0 / a.hz)

    print("inputlog: capture written to %s (%.1f s)" % (a.out, time.perf_counter() - t0))


if __name__ == "__main__":
    main()
