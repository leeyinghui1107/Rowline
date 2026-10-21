#!/usr/bin/env python3
"""Parse PE MENU (RT_MENU, MENUITEMTEMPLATE) and ACCELERATOR (RT_ACCELERATOR) resources."""
import struct

def read_wstr(d, off):
    n = struct.unpack_from("<H", d, off)[0]; off += 2
    s = d[off:off+n*2].decode("utf-16-le", "replace"); off += n*2
    return s, off

def parse_menu(d, off=0, depth=0, out=None):
    if out is None: out = []
    option, mid = struct.unpack_from("<HH", d, off); off += 4
    first = struct.unpack_from("<H", d, off)[0]
    if first == 0xFFFF:
        off += 2
        res = struct.unpack_from("<H", d, off)[0]; off += 2
        label = f"<res #{res}>"
    else:
        label, off = read_wstr(d, off)
    if option & 0x0010:   # MF_POPUP
        out.append(("  "*depth) + f"POPUP  {label}")
        while struct.unpack_from("<H", d, off)[0] != 0:
            off = parse_menu(d, off, depth+1, out)
        off += 2          # terminator
    else:
        flag = []
        if option & 0x0001: flag.append("GRAYED")
        if option & 0x0002: flag.append("CHECKED")
        if option & 0x4000: flag.append("POPUP")
        out.append(("  "*depth) + f"ITEM   id={mid:<5} {label}" + ("  [" + ",".join(flag) + "]" if flag else ""))
    return off

def parse_accel(d):
    out, off = [], 0
    while off + 8 <= len(d):
        fvirt, key, cmd, _pad = struct.unpack_from("<HHHH", d, off); off += 8
        if fvirt == 0 and key == 0 and cmd == 0: break
        ctrl = (fvirt & 0x0F) == 0x0F
        shift = fvirt & 0x04
        alt = fvirt & 0x10
        noinv = fvirt & 0x80
        mods = ("Ctrl+" if ctrl else "") + ("Shift+" if shift else "") + ("Alt+" if alt else "")
        if 0x70 <= key <= 0x87: kn = f"F{key-0x6F}"
        elif 0x30 <= key <= 0x7A: kn = chr(key)
        elif key == 0x1B: kn = "Esc"
        elif key == 0x0D: kn = "Enter"
        elif key == 0x20: kn = "Space"
        else: kn = f"0x{key:02X}"
        out.append((cmd, mods+kn, bool(noinv)))
    return out

if __name__ == "__main__":
    m = open("/tmp/wl_res/MENU_1100_0", "rb").read()
    ver, hdr_off = struct.unpack_from("<HH", m, 0)
    print(f"== MENU_1100  version={ver} firstItemOffset={hdr_off}")
    lines = []
    parse_menu(m, hdr_off, 0, lines)
    for l in lines: print("  " + l)
    a = open("/tmp/wl_res/ACCELERATOR_1100_0", "rb").read()
    print("== ACCELERATOR_1100")
    for cmd, key, noinv in parse_accel(a):
        print(f"   cmd={cmd:<5} -> {key}")
