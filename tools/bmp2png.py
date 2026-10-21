#!/usr/bin/env python3
"""Decode PE BMP resources (DIB form: no BITMAPFILEHEADER) and write PNG for inspection."""
import struct, sys, zlib, os

def png_write(path, w, h, rgb):
    raw = b"".join(b"\x00" + rgb[y*w*3:(y+1)*w*3] for y in range(h))
    def chunk(t, d):
        return struct.pack(">I", len(d)) + t + d + struct.pack(">I", zlib.crc32(t+d) & 0xffffffff)
    open(path, "wb").write(b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0))
        + chunk(b"IDAT", zlib.compress(raw, 9)) + chunk(b"IEND", b""))

def decode_bmp(d):
    (bsz, w, h, planes, bpp, comp, imgsz, xppm, yppm, clrused, clrimp) = \
        struct.unpack_from("<IiiHHIIiiII", d, 0)
    assert bsz == 40, bsz
    ncol = clrused if clrused else (1 << bpp if bpp <= 8 else 0)
    pal = []
    po = 40
    for i in range(ncol):
        b, g, r, _ = d[po+i*4:po+i*4+4]
        pal.append((r, g, b))
    while len(pal) < 256: pal.append((0,0,0))
    off = po + ncol*4
    flip = h > 0
    h = abs(h)
    info = dict(w=w, h=h, bpp=bpp, comp=comp, planes=planes, off=off, ncol=ncol,
                clrused=clrused, imgsz=imgsz, xppm=xppm, yppm=yppm)
    if comp == 0:
        stride = ((w*bpp+31)//32)*4
        rows = [d[off+y*stride: off+(y+1)*stride] for y in range(h)]
        if flip: rows.reverse()
        out = bytearray()
        for r in rows:
            if bpp == 24:
                for x in range(w):
                    b,g,rr = r[x*3], r[x*3+1], r[x*3+2]
                    out += bytes((rr,g,b))
            elif bpp == 8:
                out += b"".join(bytes(pal[px]) for px in r[:w])
            elif bpp == 4:
                for x in range(w):
                    byte = r[x//2]
                    v = (byte >> 4) if x % 2 == 0 else (byte & 0xF)
                    out += bytes(pal[v])
            elif bpp == 1:
                for x in range(w):
                    v = (r[x//8] >> (7-(x%8))) & 1
                    out += bytes(pal[v])
        return bytes(out), info
    if comp in (1, 2):   # BI_RLE8 / BI_RLE4
        bits = 8 if comp == 1 else 4
        px = [[0]*w for _ in range(h)]
        x = y = 0
        i = off
        end = len(d)
        while i < end-1:
            a, b = d[i], d[i+1]; i += 2
            if a > 0:
                for k in range(a):
                    v = b if bits == 8 else ((b >> 4) if k % 2 == 0 else (b & 0xF))
                    if x < w and y < h: px[y][x] = v
                    x += 1
            elif a == 0:
                if b == 0: x = 0; y += 1
                elif b == 1: break
                elif b == 2: x += d[i]; y += d[i+1]; i += 2
                else:
                    cnt = b
                    if bits == 8:
                        for k in range(cnt):
                            if i+k < end and x < w and y < h: px[y][x] = d[i+k]
                            x += 1
                        i += cnt
                    else:
                        nbytes = (cnt+1)//2
                        for k in range(nbytes):
                            byte = d[i+k]
                            for j in range(2):
                                if x < w and y < h: px[y][x] = (byte>>4) if j == 0 else (byte & 0xF)
                                x += 1
                        i += nbytes
                    if nbytes_odd := (cnt & 1): i += 1
        if flip: px.reverse()
        out = bytearray()
        for r in px:
            out += b"".join(bytes(pal[v]) for v in r)
        return bytes(out), info
    return None, info

if __name__ == "__main__":
    os.makedirs("/tmp/wl_png", exist_ok=True)
    for f in sorted(os.listdir("/tmp/wl_res")):
        if not f.startswith("BITMAP"): continue
        d = open("/tmp/wl_res/"+f, "rb").read()
        try:
            rgb, info = decode_bmp(d)
        except Exception as e:
            print(f, "ERR", e); continue
        if rgb is None:
            print(f"{f}: comp={info['comp']} unsupported"); continue
        p = "/tmp/wl_png/" + f + ".png"
        png_write(p, info["w"], info["h"], rgb)
        print(f"{f}: {info['w']}x{info['h']} bpp={info['bpp']} comp={info['comp']} "
              f"pal={info['ncol']} imgsz={info['imgsz']} -> {p}")
