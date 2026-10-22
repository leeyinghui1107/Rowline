#!/usr/bin/env python3
"""
Check that the committed assets match the generators.

CI runs this after `tools/make_sheet.py` and `tools/make_icons.py`:

    python3 tools/check_assets.py

The comparison is done on **decoded pixels**, not on file bytes.  Different
zlib builds can compress the same pixels into different bytes, which made a
plain `git diff --exit-code` fail for reasons that are not a real regression.
SVG sources are compared as text (the generators format numbers with a fixed
precision for that reason).
"""
import os
import struct
import subprocess
import sys
import zlib

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), ".."))
DIRS = ("assets/sheet", "assets/icons", "assets/noise")


def git(*args):
    return subprocess.check_output(["git", *args], cwd=ROOT)


def head_files():
    """Tracked asset paths in HEAD."""
    out = git("ls-tree", "-r", "--name-only", "HEAD", *DIRS).decode("utf-8")
    return [line for line in out.splitlines() if line]


def head_bytes(path):
    return git("show", "HEAD:" + path)


def decode_png(data):
    """Return (width, height, channels, raw scanlines) with all channels.

    Only the subset this project writes is supported: 8-bit, non-interlaced,
    colour types 0/2/3/4/6.
    """
    if data[:8] != b"\x89PNG\r\n\x1a\n":
        raise ValueError("not a PNG")
    pos = 8
    w = h = depth = ctype = interlace = 0
    idat = b""
    palette = b""
    while pos < len(data):
        (length,) = struct.unpack_from(">I", data, pos)
        kind = data[pos + 4:pos + 8]
        body = data[pos + 8:pos + 8 + length]
        pos += 12 + length
        if kind == b"IHDR":
            w, h, depth, ctype, _comp, _filt, interlace = struct.unpack(">IIBBBBB", body)
        elif kind == b"PLTE":
            palette = body
        elif kind == b"IDAT":
            idat += body
        elif kind == b"IEND":
            break
    if depth != 8 or interlace:
        raise ValueError("unsupported PNG (depth %d, interlace %d)" % (depth, interlace))
    channels = {0: 1, 2: 3, 3: 1, 4: 2, 6: 4}[ctype]
    raw = zlib.decompress(idat)
    stride = w * channels
    rows = []
    prev = bytearray(stride)
    p = 0
    for _ in range(h):
        filt = raw[p]
        p += 1
        line = bytearray(raw[p:p + stride])
        p += stride
        if filt == 1:
            for i in range(channels, stride):
                line[i] = (line[i] + line[i - channels]) & 0xFF
        elif filt == 2:
            for i in range(stride):
                line[i] = (line[i] + prev[i]) & 0xFF
        elif filt == 3:
            for i in range(stride):
                a = line[i - channels] if i >= channels else 0
                line[i] = (line[i] + ((a + prev[i]) >> 1)) & 0xFF
        elif filt == 4:
            for i in range(stride):
                a = line[i - channels] if i >= channels else 0
                c = prev[i - channels] if i >= channels else 0
                b = prev[i]
                pa, pb, pc = abs(b - c), abs(a - c), abs(a + b - 2 * c)
                pr = a if (pa <= pb and pa <= pc) else (b if pb <= pc else c)
                line[i] = (line[i] + pr) & 0xFF
        elif filt != 0:
            raise ValueError("bad filter %d" % filt)
        rows.append(bytes(line))
        prev = line
    if ctype == 3:
        rgb = bytearray()
        for line in rows:
            for index in line:
                rgb += palette[index * 3:index * 3 + 3]
        return w, h, 3, bytes(rgb)
    return w, h, channels, b"".join(rows)


def compare(path):
    """None when equal, else a human-readable reason."""
    current = os.path.join(ROOT, path)
    if not os.path.exists(current):
        return "missing"
    ours = open(current, "rb").read()
    theirs = head_bytes(path)
    if path.endswith(".svg"):
        # The checkout may use CRLF while `git show` yields the LF blob.
        def norm(b):
            return b.replace(b"\r\n", b"\n")
        return None if norm(ours) == norm(theirs) else "SVG text differs"
    if not path.endswith(".png"):
        return None if ours == theirs else "file differs"
    a = decode_png(ours)
    b = decode_png(theirs)
    if a[:3] != b[:3]:
        return "size/channels %s vs %s" % (a[:3], b[:3])
    if a[3] != b[3]:
        diff = sum(1 for x, y in zip(a[3], b[3]) if x != y)
        return "%d byte(s) differ" % diff
    return None


def main():
    files = head_files()
    if not files:
        print("check_assets: no tracked assets found")
        return 1
    bad = []
    for path in files:
        try:
            reason = compare(path)
        except Exception as exc:  # noqa: BLE001 - report and keep checking
            reason = "cannot compare: %s" % exc
        if reason:
            bad.append((path, reason))
    if bad:
        print("asset mismatch after regeneration:")
        for path, reason in bad:
            print("  %-48s %s" % (path, reason))
        return 1
    print("assets match the generators (%d files)" % len(files))
    return 0


if __name__ == "__main__":
    sys.exit(main())
