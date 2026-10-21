#!/usr/bin/env python3
"""
Placeholder sprite-sheet generator + artist guide overlay.

Emits a 576x586 24-bit sheet that uses EXACTLY the same index layout as the
original, so the renderer code (and the behaviour spec) stay unchanged:

    source X = cellValue * 36 + spriteSet * 288
    source Y = 225

    columns  0..7   spriteSet 0 : col 0 = empty cell, cols 1..7 = spheres
    columns  8..15  spriteSet 1 : col 8 = empty cell, cols 9..15 = polygons

Outputs
    assets/sheet/rowline_sheet.png     placeholder art (replaceable)
    assets/sheet/rowline_sheet_guide.png  grid + labels for the artist

The palette matches the colours recovered from the original atlas.
"""
import math, os, struct, zlib

# The atlas geometry at scale 1.  Every coordinate is multiplied by `scale`
# so that an @2x / @3x sheet is a genuine high-resolution rendering rather
# than an upscaled copy.
W1, H1 = 576, 586
CELL1 = 36
INSET1 = 2
BAND_Y1 = 225         # the row the renderer actually samples
SPRITE1 = 32          # drawn size inside the 36 px cell

BG = (192, 192, 192)   # Win95 button face

# colour -> (r, g, b); recovered from the original atlas histogram
COLORS = [
    (0, 160, 0),      # 1 green
    (0, 0, 192),      # 2 blue
    (0, 192, 192),    # 3 cyan
    (192, 0, 192),    # 4 magenta
    (192, 0, 0),      # 5 red
    (192, 192, 0),    # 6 yellow
    (128, 0, 0),      # 7 maroon
]

# per-colour polygon used in "figure" mode (matches the original's layout)
SHAPES = ["square", "square", "diamond", "tri_up", "tri_down", "tri_right", "tri_right"]


class Canvas:
    def __init__(self, w, h, fill=BG):
        self.w, self.h = w, h
        self.px = bytearray(w * h * 3)
        for i in range(w * h):
            self.px[i*3:i*3+3] = bytes(fill)

    def set(self, x, y, c):
        if 0 <= x < self.w and 0 <= y < self.h:
            o = (y * self.w + x) * 3
            self.px[o:o+3] = bytes(c)

    def blend(self, x, y, c, a):
        if not (0 <= x < self.w and 0 <= y < self.h) or a <= 0:
            return
        o = (y * self.w + x) * 3
        for k in range(3):
            old = self.px[o+k]
            self.px[o+k] = int(old + (c[k] - old) * a)

    def png(self, path):
        raw = b"".join(b"\x00" + bytes(self.px[y*self.w*3:(y+1)*self.w*3])
                       for y in range(self.h))
        def chunk(t, d):
            return struct.pack(">I", len(d)) + t + d + struct.pack(">I", zlib.crc32(t+d) & 0xffffffff)
        open(path, "wb").write(
            b"\x89PNG\r\n\x1a\n"
            + chunk(b"IHDR", struct.pack(">IIBBBBB", self.w, self.h, 8, 2, 0, 0, 0))
            + chunk(b"IDAT", zlib.compress(raw, 9))
            + chunk(b"IEND", b""))
        print("wrote %s (%dx%d)" % (path, self.w, self.h))

    def bmp(self, path):
        """24-bit BMP (bottom-up) so SDL2 core can load it without SDL2_image."""
        stride = ((self.w * 3 + 3) // 4) * 4
        pad = stride - self.w * 3
        pixels = bytearray()
        for y in range(self.h - 1, -1, -1):
            row = self.px[y*self.w*3:(y+1)*self.w*3]
            pixels += bytes(b for i in range(self.w) for b in (row[i*3+2], row[i*3+1], row[i*3]))
            pixels += b"\x00" * pad
        off = 54
        size = off + len(pixels)
        hdr = b"BM" + struct.pack("<IHHI", size, 0, 0, off)
        hdr += struct.pack("<IiiHHIIiiII", 40, self.w, self.h, 1, 24, 0, len(pixels),
                           2834, 2834, 0, 0)
        open(path, "wb").write(hdr + bytes(pixels))
        print("wrote %s (%dx%d)" % (path, self.w, self.h))


def shade(c, f):
    return (max(0, min(255, int(c[0]*f))), max(0, min(255, int(c[1]*f))), max(0, min(255, int(c[2]*f))))


def draw_sphere(cv, cx, cy, rad, col):
    """Sphere with a highlight and a rim shadow."""
    for y in range(cy-rad-1, cy+rad+2):
        for x in range(cx-rad-1, cx+rad+2):
            dx, dy = x - cx, y - cy
            d = math.hypot(dx, dy)
            if d > rad + 0.5:
                continue
            if d > rad - 0.5:
                cv.blend(x, y, shade(col, 0.55), 1.0 - (d - (rad - 0.5)))
                continue
            nz = math.sqrt(max(0.0, 1.0 - (d/rad)**2))
            lx, ly = dx - rad*0.35, dy - rad*0.4
            ln = math.hypot(lx, ly) or 1.0
            lam = max(0.0, (lx*lx + ly*ly) / (ln*ln) * 0.35 + 0.65*nz)
            c = shade(col, 0.45 + 0.75*lam)
            cv.set(x, y, c)
            if d < rad*0.45:
                cv.blend(x, y, (255, 255, 255), 0.30 * (1 - d/(rad*0.45)))


def draw_poly(cv, cx, cy, rad, col, shape):
    """Flat polygon with a light edge."""
    if shape == "square":
        pts = [(-rad, -rad), (rad, -rad), (rad, rad), (-rad, rad)]
    elif shape == "diamond":
        pts = [(0, -rad), (rad, 0), (0, rad), (-rad, 0)]
    elif shape == "tri_up":
        pts = [(0, -rad), (rad, rad*0.85), (-rad, rad*0.85)]
    elif shape == "tri_down":
        pts = [(0, rad), (rad, -rad*0.85), (-rad, -rad*0.85)]
    elif shape == "tri_right":
        pts = [(rad, 0), (-rad*0.85, rad), (-rad*0.85, -rad)]
    else:
        pts = [(-rad, -rad), (rad, -rad), (rad, rad), (-rad, rad)]

    def inside(px, py):
        n = len(pts)
        for i in range(n):
            x1, y1 = pts[i]
            x2, y2 = pts[(i+1) % n]
            if (y2 > py) != (y1 > py):
                xint = (x2-x1) * (py-y1) / (y2-y1 + 1e-9) + x1
                if px < xint:
                    return False
        return True

    for y in range(cy-rad-1, cy+rad+2):
        for x in range(cx-rad-1, cx+rad+2):
            if inside(x-cx, y-cy):
                cv.set(x, y, col)
    # light edge
    for i in range(len(pts)):
        x1, y1 = pts[i]; x2, y2 = pts[(i+1) % len(pts)]
        steps = int(max(abs(x2-x1), abs(y2-y1)) * 2) + 1
        for s in range(steps+1):
            t = s / steps
            cv.set(int(cx + x1 + (x2-x1)*t), int(cy + y1 + (y2-y1)*t), shade(col, 1.45))


def cell_origin(col, scale):
    return (col * CELL1 * scale, BAND_Y1 * scale)


def build_sheet(scale=1):
    """Render the atlas at `scale` times the base resolution."""
    CELL = CELL1 * scale
    INSET = INSET1 * scale
    SPRITE = SPRITE1 * scale
    cv = Canvas(W1 * scale, H1 * scale)
    for col in range(16):
        x0, y0 = cell_origin(col, scale)
        # empty-cell tile (columns 0 and 8)
        if col in (0, 8):
            for y in range(y0, y0+CELL):
                for x in range(x0, x0+CELL):
                    edge = (x < x0+scale or y < y0+scale
                            or x >= x0+CELL-scale or y >= y0+CELL-scale)
                    cv.set(x, y, (160, 160, 160) if edge else BG)
            continue
        if col <= 7:
            c = COLORS[col-1]
            draw_sphere(cv, x0+CELL//2, y0+CELL//2, max(1, SPRITE//2 - scale), c)
        else:
            c = COLORS[col-9]
            draw_poly(cv, x0+CELL//2, y0+CELL//2, max(1, SPRITE//2 - 2*scale),
                      c, SHAPES[col-9])
    return cv


def build_guide(scale=1):
    """Same sheet with grid lines and index labels baked in as tick marks."""
    CELL = CELL1 * scale
    INSET = INSET1 * scale
    cv = build_sheet(scale)
    for col in range(16):
        x0, y0 = cell_origin(col, scale)
        for k in range(CELL):
            cv.set(x0+k, y0, (255, 255, 255))
            cv.set(x0+k, y0+CELL-1, (255, 255, 255))
    for y in range(0, H1*scale, CELL):
        for k in range(0, W1*scale, 4*scale):
            cv.set(k, y, (255, 255, 255))
    # corner marks for the sampled band
    for col in range(16):
        x0, y0 = cell_origin(col, scale)
        for k in range(INSET):
            cv.set(x0+k, y0+INSET+k, (255, 0, 0))
            cv.set(x0+CELL-1-k, y0+INSET+k, (255, 0, 0))
    return cv


if __name__ == "__main__":
    root = os.path.abspath(os.path.join(os.path.dirname(__file__), ".."))
    out = os.path.join(root, "assets", "sheet")
    os.makedirs(out, exist_ok=True)
    for scale in (1, 2, 3):
        base = "rowline_sheet" if scale == 1 else "rowline_sheet@%dx" % scale
        build_sheet(scale).png(os.path.join(out, base + ".png"))
        build_sheet(scale).bmp(os.path.join(out, base + ".bmp"))
        if scale == 1:
            build_guide(scale).png(os.path.join(out, "rowline_sheet_guide.png"))
    print("\nindex map (renderer formula: srcX = value*36 + set*288, srcY = 225)")
    print("  value 0 / set 0 -> col  0  empty cell")
    print("  value 1..7 / set 0 -> cols 1..7  spheres")
    print("  value 0 / set 1 -> col  8  empty cell")
    print("  value 1..7 / set 1 -> cols 9..15 polygons")
