#!/usr/bin/env python3
"""
Generate the icon set and the tiling noise texture.

Icons follow docs/10 section 3.1:
  * a 24 x 24 grid, 2 px safe margin, the glyph inside 20 x 20;
  * 2 px strokes with round caps;
  * **single colour** -- the PNG is white with an alpha mask, so the runtime
    can tint it to any theme colour;
  * exported at @1x / @2x / @3x / @4x;
  * an SVG per icon is written next to the PNGs as the artist's source.

The noise texture is a seamlessly tileable 256 x 256 grain used at a very low
alpha to take the "plastic" look off large flat areas.

Usage:
    python3 tools/make_icons.py
    python3 tools/make_icons.py --noise-only
"""
import math
import os
import sys

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), ".."))
OUT = os.path.join(ROOT, "assets", "icons")
NOISE_OUT = os.path.join(ROOT, "assets", "noise")

GRID = 24.0          # the design grid, in units
STROKE = 2.0         # the stroke width, in units
SUPERSAMPLE = 4      # anti-aliasing factor

# Quantise every coordinate that comes out of a transcendental function.
# `math.cos`/`hypot` may differ in their last bit between platforms and
# Python/libm versions, which is enough to flip an anti-aliased pixel; the
# CI job compares the regenerated PNGs byte for byte.  Six decimals is far
# below one pixel and makes the output reproducible.
PRECISION = 6


def q(v):
    r = round(v, PRECISION)
    return 0.0 if r == 0 else r

# --------------------------------------------------------------------- canvas
class Mask:
    """An 8-bit coverage mask."""

    def __init__(self, w, h):
        self.w, self.h = w, h
        self.px = bytearray(w * h)

    def blend(self, x, y, a):
        if 0 <= x < self.w and 0 <= y < self.h:
            i = y * self.w + x
            v = self.px[i] + a
            self.px[i] = 255 if v > 255 else v

    def to_rgba(self):
        out = bytearray(self.w * self.h * 4)
        for i in range(self.w * self.h):
            out[i * 4:i * 4 + 4] = bytes((255, 255, 255, self.px[i]))
        return bytes(out)

    def png(self, path):
        raw = b"".join(b"\x00" + self.to_rgba()[y * self.w * 4:(y + 1) * self.w * 4]
                       for y in range(self.h))
        import struct, zlib
        def chunk(t, d):
            return struct.pack(">I", len(d)) + t + d + struct.pack(">I", zlib.crc32(t + d) & 0xffffffff)
        open(path, "wb").write(
            b"\x89PNG\r\n\x1a\n"
            + chunk(b"IHDR", struct.pack(">IIBBBBB", self.w, self.h, 8, 6, 0, 0, 0))
            + chunk(b"IDAT", zlib.compress(raw, 9))
            + chunk(b"IEND", b""))
        print("wrote %s (%dx%d)" % (path, self.w, self.h))


class Shape:
    """Draws into a Mask, in grid units."""

    def __init__(self, mask, scale):
        self.m = mask
        self.s = scale

    def _pt(self, x, y):
        return q(x * self.s), q(y * self.s)

    def line(self, x0, y0, x1, y1, w=STROKE):
        """A round-capped thick line."""
        half = w * self.s / 2.0
        ax, ay = self._pt(x0, y0)
        bx, by = self._pt(x1, y1)
        dx, dy = q(bx - ax), q(by - ay)
        length = q(math.hypot(dx, dy))
        steps = max(2, int(length / 0.35))
        for i in range(steps + 1):
            t = i / steps
            self._disc(q(ax + dx * t), q(ay + dy * t), half)

    def _disc(self, cx, cy, r):
        cx, cy, r = q(cx), q(cy), q(r)
        x0, x1 = int(cx - r - 1), int(cx + r + 2)
        y0, y1 = int(cy - r - 1), int(cy + r + 2)
        for y in range(y0, y1):
            for x in range(x0, x1):
                d = q(math.hypot(x + 0.5 - cx, y + 0.5 - cy))
                if d <= r:
                    self.m.blend(x, y, 255)
                elif d <= r + 1.0:
                    self.m.blend(x, y, int(255 * (r + 1.0 - d)))

    def rect(self, x, y, w, h, radius=0.0, filled=False):
        """
        A rounded rectangle.

        The default is an **outline** with the standard stroke width, because
        the icon set is a line-icon set; `filled=True` gives a solid block.
        """
        r = min(radius, w / 2.0, h / 2.0)
        if filled:
            px0, py0 = self._pt(x, y)
            px1, py1 = self._pt(x + w, y + h)
            for py in range(int(py0), int(py1 + 1)):
                for px in range(int(px0), int(px1 + 1)):
                    self.m.blend(px, py, 255)
            return
        if r <= 0:
            self.line(x, y, x + w, y)
            self.line(x + w, y, x + w, y + h)
            self.line(x + w, y + h, x, y + h)
            self.line(x, y + h, x, y)
            return
        # four edges
        self.line(x + r, y, x + w - r, y)
        self.line(x + w, y + r, x + w, y + h - r)
        self.line(x + w - r, y + h, x + r, y + h)
        self.line(x, y + h - r, x, y + r)
        # four quarter arcs
        self.arc(x + r, y + r, r, math.pi, math.pi * 1.5)
        self.arc(x + w - r, y + r, r, math.pi * 1.5, math.pi * 2.0)
        self.arc(x + w - r, y + h - r, r, 0.0, math.pi * 0.5)
        self.arc(x + r, y + h - r, r, math.pi * 0.5, math.pi)

    def circle(self, cx, cy, r, filled=False):
        px, py = self._pt(cx, cy)
        rr = q(r * self.s)
        for y in range(int(py - rr - 1), int(py + rr + 2)):
            for x in range(int(px - rr - 1), int(px + rr + 2)):
                d = q(math.hypot(x + 0.5 - px, y + 0.5 - py))
                if filled:
                    if d <= rr:
                        self.m.blend(x, y, 255)
                    elif d <= rr + 1.0:
                        self.m.blend(x, y, int(255 * (rr + 1.0 - d)))
                else:
                    if abs(d - rr) <= 1.0:
                        self.m.blend(x, y, 255)

    def disc(self, cx, cy, r):
        """A filled dot, in grid units."""
        self._disc(*self._pt(cx, cy), r * self.s)

    def arc(self, cx, cy, r, a0, a1):
        px, py = self._pt(cx, cy)
        rr = q(r * self.s)
        steps = max(8, int(abs(a1 - a0) * rr / 2))
        for i in range(steps + 1):
            a = a0 + (a1 - a0) * i / steps
            self._disc(q(px + math.cos(a) * rr), q(py + math.sin(a) * rr), 1.2)

    def poly(self, points, filled=True):
        # `points` are given in grid units, so they must be scaled to pixels
        # before they are rasterised -- otherwise the shape keeps its size at
        # every export factor.
        pts = [self._pt(x, y) for x, y in points]
        if filled:
            ys = [p[1] for p in pts]
            xs = [p[0] for p in pts]
            for y in range(int(min(ys)), int(max(ys) + 1)):
                for x in range(int(min(xs)), int(max(xs) + 1)):
                    if inside(pts, x + 0.5, y + 0.5):
                        self.m.blend(x, y, 255)
        else:
            for i in range(len(pts)):
                a = pts[i]
                b = pts[(i + 1) % len(pts)]
                self.line(a[0], a[1], b[0], b[1], 1.6)

    def polyline(self, points, w=STROKE):
        for i in range(len(points) - 1):
            a, b = points[i], points[i + 1]
            self.line(a[0], a[1], b[0], b[1], w)


def inside(pts, x, y):
    n = len(pts)
    c = False
    j = n - 1
    for i in range(n):
        xi, yi = pts[i]
        xj, yj = pts[j]
        if ((yi > y) != (yj > y)) and (x < (xj - xi) * (y - yi) / (yj - yi + 1e-9) + xi):
            c = not c
        j = i
    return c


# --------------------------------------------------------------------- icons
def icon_new(s):
    s.rect(6, 3, 10, 18, 2)
    s.line(11, 8, 11, 16)
    s.line(15, 8, 15, 13)


def icon_trophy(s):
    # the cup: a trapezoid outline
    s.polyline([(7, 3), (17, 3), (15.5, 10), (8.5, 10), (7, 3)], 2.0)
    # the handles
    s.arc(7, 6.5, 2.6, math.pi * 0.5, math.pi * 1.5)
    s.arc(17, 6.5, 2.6, math.pi * 0.5, math.pi * 1.5)
    # stem and base
    s.line(12, 10, 12, 16)
    s.line(8, 19, 16, 19)
    s.line(9.5, 16, 14.5, 16)


def icon_exit(s):
    s.rect(4, 4, 10, 16, 2)
    s.line(14, 12, 21, 12)
    s.polyline([(18, 9), (21, 12), (18, 15)], 2)


def icon_next(s):
    s.polyline([(7, 6), (12, 12), (7, 18)], 2.2)
    s.polyline([(13, 6), (18, 12), (13, 18)], 2.2)


def icon_stats(s):
    s.line(4, 20, 20, 20)
    s.rect(6, 13, 3, 7)
    s.rect(11, 9, 3, 11)
    s.rect(16, 5, 3, 15)


def icon_figures(s):
    s.circle(8, 8, 4.2, True)
    s.rect(13, 13, 8, 8, 1.6)


def icon_help(s):
    s.circle(12, 12, 9)
    # the question mark: an arc that curls into a stem
    s.arc(12, 9.5, 3.0, math.pi * 1.0, math.pi * 2.05)
    s.line(12, 11.6, 12, 14.2)
    s.disc(12, 17.2, 1.25)


def icon_contents(s):
    s.polyline([(4, 5), (4, 19), (12, 19), (12, 5), (4, 5)], 1.8)
    s.line(12, 8, 20, 8)
    s.line(12, 12, 18, 12)
    s.line(12, 16, 16, 16)


def icon_about(s):
    s.circle(12, 12, 9)
    s.line(12, 11, 12, 16)
    s.disc(*s._pt(12, 7.5), 1.3)


def icon_settings(s):
    s.circle(12, 12, 3.4)
    for i in range(8):
        a = i * math.pi / 4
        s.line(12 + math.cos(a) * 6.5, 12 + math.sin(a) * 6.5,
               12 + math.cos(a) * 9.5, 12 + math.sin(a) * 9.5, 2.0)


def icon_crown(s):
    """The king's crown -- the mark of the CPU player.

    Filled, because a 20 px outline crown closes up into a blob; the rest
    of the set still reads as line art.
    """
    s.poly([(4, 19), (6, 6), (9.5, 11.5), (12, 4.5), (14.5, 11.5), (18, 6),
            (20, 19)], True)
    # the band under the points
    s.rect(3.5, 19, 17, 2.6, 0.8, True)


def icon_sword(s):
    """The challenger's sword -- the mark of the human player."""
    # the blade: a long, tapered point
    s.poly([(12, 1.5), (14.5, 5.5), (14.5, 13.5), (9.5, 13.5), (9.5, 5.5)], True)
    # the crossguard
    s.line(7.5, 13.5, 16.5, 13.5, 2.0)
    # the grip and pommel
    s.line(12, 13.5, 12, 18.5)
    s.disc(12, 19.4, 1.35)


def icon_theme(s):
    s.circle(12, 12, 8.5)
    s.poly([(12, 3.5), (12, 20.5), (19, 12)], True)


ICONS = [
    ("new", icon_new),
    ("trophy", icon_trophy),
    ("exit", icon_exit),
    ("next", icon_next),
    ("stats", icon_stats),
    ("figures", icon_figures),
    ("help", icon_help),
    ("contents", icon_contents),
    ("about", icon_about),
    ("settings", icon_settings),
    ("theme", icon_theme),
    ("crown", icon_crown),
    ("sword", icon_sword),
]

SVG_HEAD = ('<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" '
            'width="24" height="24" fill="none" stroke="currentColor" '
            'stroke-width="2" stroke-linecap="round" stroke-linejoin="round">\n')


def fnum(v):
    """A stable decimal representation (no platform-dependent repr)."""
    s = ("%.*f" % (PRECISION, v)).rstrip("0").rstrip(".")
    return s if s not in ("", "-0") else "0"


def svg_for(name, fn):
    """A rough SVG mirror of the raster icon (the artist reference)."""
    parts = []
    lines = []
    rects = []
    circles = []

    class SvgShape(Shape):
        def line(self, x0, y0, x1, y1, w=STROKE):
            lines.append(f'<line x1="{fnum(x0)}" y1="{fnum(y0)}" x2="{fnum(x1)}" '
                          f'y2="{fnum(y1)}" stroke-width="{fnum(w)}"/>')

        def rect(self, x, y, w, h, radius=0.0, filled=False):
            fill = 'currentColor' if filled else 'none'
            rects.append(f'<rect x="{fnum(x)}" y="{fnum(y)}" width="{fnum(w)}" '
                         f'height="{fnum(h)}" rx="{fnum(radius)}" fill="{fill}"/>')

        def circle(self, cx, cy, r, filled=False):
            circles.append(f'<circle cx="{fnum(cx)}" cy="{fnum(cy)}" r="{fnum(r)}"/>')

        def poly(self, points, filled=True):
            pts = " ".join(f"{fnum(x)},{fnum(y)}" for x, y in points)
            fill = 'currentColor' if filled else 'none'
            parts.append(f'<polygon points="{pts}" fill="{fill}"/>')

        def polyline(self, points, w=STROKE):
            pts = " ".join(f"{fnum(x)},{fnum(y)}" for x, y in points)
            parts.append(f'<polyline points="{pts}" stroke-width="{fnum(w)}"/>')

        def arc(self, cx, cy, r, a0, a1):
            pass

        def disc(self, cx, cy, r):
            circles.append(f'<circle cx="{fnum(cx)}" cy="{fnum(cy)}" r="{fnum(r)}" '
                           f'fill="currentColor" stroke="none"/>')

        def _disc(self, cx, cy, r):
            pass

    s = SvgShape(Mask(1, 1), 1.0)
    fn(s)
    body = "\n".join(lines + rects + circles + parts)
    return SVG_HEAD + body + "\n</svg>\n"


def render_icon(fn, size):
    """Rasterise one icon at `size` pixels (24, 48 or 72)."""
    scale = size / GRID
    mask = Mask(size, size)
    s = Shape(mask, scale)
    fn(s)
    return mask


def build_strip(scale):
    """A horizontal strip of all the icons at the given scale."""
    size = int(GRID * scale)
    strip = Mask(size * len(ICONS), size)
    for i, (_, fn) in enumerate(ICONS):
        icon = render_icon(fn, size)
        for y in range(size):
            for x in range(size):
                a = icon.px[y * size + x]
                if a:
                    strip.blend(i * size + x, y, a)
    return strip


def build_noise(size=256, seed=1337):
    """A seamlessly tileable grain texture."""
    import random
    rnd = random.Random(seed)
    grid = 32
    cell = size // grid
    base = [[rnd.random() for _ in range(grid)] for _ in range(grid)]
    out = bytearray(size * size)
    for y in range(size):
        gy = y / cell
        y0 = int(gy) % grid
        y1 = (y0 + 1) % grid
        fy = gy - int(gy)
        for x in range(size):
            gx = x / cell
            x0 = int(gx) % grid
            x1 = (x0 + 1) % grid
            fx = gx - int(gx)
            # bilinear, wrapping -> seamless
            v = (base[y0][x0] * (1 - fx) * (1 - fy) + base[y0][x1] * fx * (1 - fy)
                 + base[y1][x0] * (1 - fx) * fy + base[y1][x1] * fx * fy)
            out[y * size + x] = int(v * 255)
    return out


def write_noise_png(path, data, size):
    import struct, zlib
    raw = b"".join(b"\x00" + bytes(data[y * size:(y + 1) * size]) for y in range(size))
    def chunk(t, d):
        return struct.pack(">I", len(d)) + t + d + struct.pack(">I", zlib.crc32(t + d) & 0xffffffff)
    open(path, "wb").write(
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", struct.pack(">IIBBBBB", size, size, 8, 0, 0, 0, 0))
        + chunk(b"IDAT", zlib.compress(raw, 9))
        + chunk(b"IEND", b""))
    print("wrote %s (%dx%d)" % (path, size, size))


if __name__ == "__main__":
    noise_only = "--noise-only" in sys.argv
    os.makedirs(OUT, exist_ok=True)
    os.makedirs(NOISE_OUT, exist_ok=True)

    if not noise_only:
        for scale, suffix in ((1, ""), (2, "@2x"), (3, "@3x"), (4, "@4x")):
            strip = build_strip(scale)
            strip.png(os.path.join(OUT, "icons%s.png" % suffix))
        # The artist's source: one SVG per icon plus a contact sheet.
        for name, fn in ICONS:
            with open(os.path.join(OUT, "%s.svg" % name), "w", encoding="utf-8") as fh:
                fh.write(svg_for(name, fn))
        print("wrote %d SVG sources to %s" % (len(ICONS), OUT))
        print("icon order: %s" % ", ".join(n for n, _ in ICONS))

    write_noise_png(os.path.join(NOISE_OUT, "noise.png"), build_noise(256), 256)
