#!/usr/bin/env python3
"""Classify the 16x10 ball-sprite grid in BITMAP_1111 using FILE coordinates (BMP is bottom-up)."""
import struct, collections, zlib, os, json

d = open("/tmp/wl_res/BITMAP_1111_0", "rb").read()
bsz, W, H, pl, bpp, comp, imgsz, xp, yp, cu, ci = struct.unpack_from("<IiiHHIIiiII", d, 0)
stride = ((W*bpp+31)//32)*4
def px(x, y):
    o = 40 + y*stride + x*3
    return (d[o+2], d[o+1], d[o])

COLS, ROWS, CELL = 16, 10, 36
BG = (192, 192, 192)
HUE = {
    "GREEN":   (0, 160, 0),   "BLUE": (0, 0, 200),   "CYAN": (0, 200, 200),
    "MAGENTA": (200, 0, 200),  "RED": (200, 0, 0),   "YELLOW": (200, 200, 0),
    "MAROON":  (140, 0, 0),
}

def cell(x0, y0):
    cnt = collections.Counter()
    for y in range(y0, y0+CELL):
        for x in range(x0, x0+CELL):
            cnt[px(x, y)] += 1
    bg = cnt.get(BG, 0)
    # pick the most saturated frequent colour -> the ball's base hue
    best, bn = None, 0
    for c, n in cnt.most_common(24):
        r, g, b = c
        sat = max(c) - min(c)
        if sat > 70 and n * (1 + sat/255) > bn:
            best, bn = c, n
    if best is None:
        if bg > CELL*CELL*0.9: return "EMPTY"
        best = cnt.most_common(1)[0][0]
    name = None
    for k, v in HUE.items():
        if abs(best[0]-v[0])+abs(best[1]-v[1])+abs(best[2]-v[2]) < 150:
            name = k; break
    if name is None:
        name = f"#{best[0]:02X}{best[1]:02X}{best[2]:02X}"
    # shape via silhouette mask occupancy
    def occ(xa, xb, ya, yb):
        n = 0
        for y in range(ya, yb):
            for x in range(xa, xb):
                if px(x, y) != BG: n += 1
        return n
    ctr = occ(x0+12, x0+24, y0+12, y0+24)
    corner = occ(x0+3, x0+12, y0+3, y0+12)
    full = occ(x0+2, x0+34, y0+2, y0+34)
    if full < 120: shape = "empty"
    elif ctr > corner * 1.5: shape = "sphere"
    elif ctr > corner * 0.75: shape = "square"
    else: shape = "tri/dia"
    return f"{name}:{shape}"

print(f"atlas {W}x{H}; ball grid {COLS}x{ROWS}, cell {CELL}px, FILE origin (0,0) (bottom-up)")
print()
table = []
for r in range(ROWS):
    row = [cell(c*CELL, r*CELL) for c in range(COLS)]
    table.append(row)
    print(f" file-row {r}: " + " ".join(f"{v:<16}" for v in row))

print("\n--- cell summary ---")
flat = [v for row in table for v in row]
for k, n in collections.Counter(flat).most_common():
    print(f"   {k:<18} x{n}")

# where are the separator bands?
print("\n--- separator / background rows (full-width gray) ---")
for r in range(ROWS):
    row = [cell(c*CELL, r*CELL) for c in range(COLS)]
    if all(v in ("EMPTY",) or v.startswith("#C0C0C0") for v in row):
        print(f"   file-row {r} (display row {ROWS-1-r}) looks like separator/empty")

json.dump({"cell": CELL, "cols": COLS, "rows": ROWS,
           "note": "file coords, bottom-up BMP; display row = rows-1-r",
           "table": table}, open("/tmp/wl_png/sprite_table.json", "w", encoding="utf-8"),
          indent=1)
print("\nwrote /tmp/wl_png/sprite_table.json")
