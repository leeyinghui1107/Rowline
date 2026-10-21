#!/usr/bin/env python3
"""Analyse BITMAP_1111 sprite atlas: find cell grid, palette, and classify each cell."""
import collections, struct, sys, zlib

def read_dib(path):
    d = open(path, "rb").read()
    (bsz, w, h, planes, bpp, comp, imgsz, xppm, yppm, clrused, clrimp) = struct.unpack_from("<IiiHHIIiiII", d, 0)
    off = 40 + (clrused if clrused else (1 << bpp if bpp <= 8 else 0))*4
    stride = ((w*bpp+31)//32)*4
    rows = [d[off+y*stride: off+(y+1)*stride] for y in range(abs(h))]
    if h > 0: rows.reverse()
    px = []
    for r in rows:
        row = []
        for x in range(w):
            row.append((r[x*3+2], r[x*3+1], r[x*3]))
        px.append(row)
    return w, h, px

def png_write(path, w, h, rgb):
    raw = b"".join(b"\x00" + rgb[y*w*3:(y+1)*w*3] for y in range(h))
    def chunk(t, d):
        return struct.pack(">I", len(d)) + t + d + struct.pack(">I", zlib.crc32(t+d) & 0xffffffff)
    open(path, "wb").write(b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0))
        + chunk(b"IDAT", zlib.compress(raw, 9)) + chunk(b"IEND", b""))

if __name__ == "__main__":
    w, h, px = read_dib("/tmp/wl_res/BITMAP_1111_0")
    print(f"atlas {w}x{h}")

    # full palette histogram
    cnt = collections.Counter()
    for row in px: cnt.update(row)
    print(f"distinct colors: {len(cnt)}")
    for c, n in cnt.most_common(18):
        print(f"   {c} #{c[0]:02X}{c[1]:02X}{c[2]:02X}  {n:>8} px ({100*n/(w*h):5.2f}%)")

    # background = most common color; find grid of "cells" by row/col projection of non-bg
    bg = cnt.most_common(1)[0][0]
    print(f"\nbackground = {bg}")
    colhit = [sum(1 for y in range(h) if px[y][x] != bg) for x in range(w)]
    rowhit = [sum(1 for x in range(w) if px[y][x] != bg) for y in range(h)]
    def runs(v, thr):
        out, s = [], None
        for i, x in enumerate(v):
            if x > thr:
                if s is None: s = i
            else:
                if s is not None: out.append((s, i-1, i-s)); s = None
        if s is not None: out.append((s, len(v)-1, len(v)-s))
        return out
    print("\ncolumn bands (non-bg):")
    for a, b, n in runs(colhit, 5): print(f"   x {a:3d}-{b:3d}  width={n}")
    print("\nrow bands (non-bg):")
    for a, b, n in runs(rowhit, 5): print(f"   y {a:3d}-{b:3d}  height={n}")

    # crop top (characters) and bottom (grid) as separate PNGs
    def crop(x0, y0, x1, y1, path, scale=1):
        cw, ch = x1-x0, y1-y0
        out = bytearray()
        for y in range(y0, y1):
            for _ in range(scale):
                pass
        buf = bytearray()
        for y in range(y0, y1):
            line = bytearray()
            for x in range(x0, x1):
                line += bytes(px[y][x])*scale
            for _ in range(scale): buf += line
        png_write(path, cw*scale, ch*scale, bytes(buf))
        print(f"   wrote {path} {cw*scale}x{ch*scale}")
    crop(0, 0, w, 235, "/tmp/wl_png/atlas_chars.png", 1)
    crop(0, 225, w, h, "/tmp/wl_png/atlas_grid.png", 1)
