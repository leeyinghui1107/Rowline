#!/usr/bin/env python3
"""
Minimal PNG reader/writer (8-bit RGB / RGBA / palette, non-interlaced).

Deliberately dependency-free so the black-box tooling runs on a bare Python
install on the analysis machine.  Supports exactly what we produce ourselves
and what common screenshot tools emit: colour types 2 (RGB), 6 (RGBA) and
3 (palette), bit depth 8, no interlace.
"""
import struct, zlib


def _paeth(a, b, c):
    p = a + b - c
    pa, pb, pc = abs(p - a), abs(p - b), abs(p - c)
    if pa <= pb and pa <= pc: return a
    if pb <= pc: return b
    return c


def read_png(path):
    """Return (width, height, rows) where rows is a list of bytearray in RGB."""
    d = open(path, "rb").read()
    if d[:8] != b"\x89PNG\r\n\x1a\n":
        raise ValueError("not a PNG: %s" % path)
    pos = 8
    w = h = depth = ctype = interlace = 0
    idat = b""
    plte = None
    trns = None
    while pos < len(d):
        ln = struct.unpack_from(">I", d, pos)[0]
        typ = d[pos+4:pos+8]
        body = d[pos+8:pos+8+ln]
        pos += 12 + ln
        if typ == b"IHDR":
            w, h, depth, ctype, _comp, _filt, interlace = struct.unpack(">IIBBBBB", body)
        elif typ == b"PLTE":
            plte = body
        elif typ == b"tRNS":
            trns = body
        elif typ == b"IDAT":
            idat += body
        elif typ == b"IEND":
            break
    if depth != 8:
        raise ValueError("only 8-bit PNGs are supported (got depth %d)" % depth)
    if interlace:
        raise ValueError("interlaced PNGs are not supported")
    nch = {0: 1, 2: 3, 3: 1, 4: 2, 6: 4}[ctype]
    raw = zlib.decompress(idat)
    stride = w * nch
    out = bytearray(w * h * 3)
    prev = bytearray(stride)
    p = 0
    for y in range(h):
        f = raw[p]; p += 1
        line = bytearray(raw[p:p+stride]); p += stride
        if f == 1:
            for i in range(nch, stride): line[i] = (line[i] + line[i-nch]) & 0xFF
        elif f == 2:
            for i in range(stride): line[i] = (line[i] + prev[i]) & 0xFF
        elif f == 3:
            for i in range(stride):
                a = line[i-nch] if i >= nch else 0
                line[i] = (line[i] + ((a + prev[i]) >> 1)) & 0xFF
        elif f == 4:
            for i in range(stride):
                a = line[i-nch] if i >= nch else 0
                c = prev[i-nch] if i >= nch else 0
                line[i] = (line[i] + _paeth(a, prev[i], c)) & 0xFF
        prev = line
        ro = y * w * 3
        if ctype == 2:
            out[ro:ro+w*3] = line
        elif ctype == 6:
            for x in range(w):
                o = ro + x*3
                out[o:o+3] = line[x*4:x*4+3]
        elif ctype == 0:
            for x in range(w):
                o = ro + x*3
                v = line[x]
                out[o] = out[o+1] = out[o+2] = v
        elif ctype == 4:
            for x in range(w):
                o = ro + x*3
                v = line[x*2]
                out[o] = out[o+1] = out[o+2] = v
        elif ctype == 3:
            for x in range(w):
                idx = line[x]
                r, g, b = plte[idx*3], plte[idx*3+1], plte[idx*3+2]
                o = ro + x*3
                out[o], out[o+1], out[o+2] = r, g, b
    rows = [out[y*w*3:(y+1)*w*3] for y in range(h)]
    return w, h, rows


def write_png(path, w, h, rows):
    raw = b"".join(b"\x00" + bytes(r) for r in rows)
    def chunk(t, d):
        return struct.pack(">I", len(d)) + t + d + struct.pack(">I", zlib.crc32(t+d) & 0xffffffff)
    open(path, "wb").write(
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0))
        + chunk(b"IDAT", zlib.compress(raw, 9))
        + chunk(b"IEND", b""))


def crop(rows, w, x0, y0, x1, y1):
    return [bytes(r[x0*3:x1*3]) for r in rows[y0:y1]]
