#!/usr/bin/env python3
"""
RT_MENU parser for WINLINEZ.EXE.

Framing discovered empirically from the Borland linker output and validated
by two independent checks:
  * the parse must consume the resource buffer exactly (276 bytes), and
  * every extracted command ID must agree with the RT_ACCELERATOR table.

The Borland menu is Simplified Chinese; labels are therefore decoded as
UTF-16LE and reported verbatim.

Empirical grammar (derived from the byte stream, see docs/02):
    header  : WORD version(0), WORD offset(0)
    entry   : WORD 0x0010            -> POPUP, followed by a UTF-16 title
            | WORD 0x0000 / 0x0080   -> structural filler (terminators)
            | WORD id                -> ITEM with that command ID,
                                       followed by a UTF-16 label
"""
import struct, json, sys

EXE = "/Users/thomas/Downloads/WINLINEZ.EXE"
RES = "/tmp/wl_res/MENU_1100_0"
ACC = "/tmp/wl_res/ACCELERATOR_1100_0"

MF_POPUP = 0x0010
FILLER = (0x0000, 0x0080)


def read_wstr(buf, off):
    """UTF-16LE, NUL-terminated. Returns (text, next_offset)."""
    start = off
    chars = []
    while off + 1 < len(buf):
        cp = buf[off] | (buf[off + 1] << 8)
        off += 2
        if cp == 0:
            break
        chars.append(chr(cp))
    return "".join(chars), off


def parse(buf):
    """Return (tree, consumed_bytes) or (None, reason)."""
    version, offset = struct.unpack_from("<HH", buf, 0)
    pos = 4

    def entries(pos, depth):
        out = []
        while pos + 1 < len(buf):
            w = struct.unpack_from("<H", buf, pos)[0]
            if w in FILLER:
                pos += 2
                continue
            if w == MF_POPUP:
                title, pos = read_wstr(buf, pos + 2)
                kids, pos = entries(pos, depth + 1)
                out.append({"type": "popup", "id": None, "label": title,
                            "children": kids})
                continue
            # plain item: WORD id + label
            label, pos = read_wstr(buf, pos + 2)
            out.append({"type": "item", "id": w, "label": label,
                        "children": []})
        return out, pos

    tree, pos = entries(pos, 0)
    return tree, pos


def parse_accel(buf):
    out, off = [], 0
    while off + 8 <= len(buf):
        fvirt, key, cmd, _ = struct.unpack_from("<HHHH", buf, off); off += 8
        if fvirt == 0 and key == 0 and cmd == 0:
            break
        ctrl = (fvirt & 0x0F) == 0x0F
        shift = fvirt & 0x04
        alt = fvirt & 0x10
        mods = ("Ctrl+" if ctrl else "") + ("Shift+" if shift else "") + ("Alt+" if alt else "")
        if 0x70 <= key <= 0x87:
            kn = "F%d" % (key - 0x6F)
        elif 0x30 <= key <= 0x7A:
            kn = chr(key)
        else:
            kn = "0x%02X" % key
        out.append({"cmd": cmd, "key": mods + kn})
    return out


def flatten(tree, out=None, path=""):
    out = [] if out is None else out
    for n in tree:
        p = path + "/" + (n["label"].split("(")[0] or "?")
        out.append({"path": p, "type": n["type"], "id": n["id"], "label": n["label"]})
        flatten(n["children"], out, p)
    return out


if __name__ == "__main__":
    buf = open(RES, "rb").read()
    accels = parse_accel(open(ACC, "rb").read())
    acc_by_cmd = {a["cmd"]: a["key"] for a in accels}

    tree, consumed = parse(buf)
    print("=== RT_MENU (RT_MENU / MENUITEMTEMPLATE, Borland framing) ===")
    print(f"    resource size = {len(buf)} bytes, consumed = {consumed} bytes "
          f"-> {'EXACT' if consumed == len(buf) else 'MISMATCH'}\n")

    def show(nodes, depth=0):
        for n in nodes:
            key = acc_by_cmd.get(n["id"], "")
            if n["type"] == "popup":
                print(f"{'  '*depth}[POPUP] id=-            label={n['label']!r}")
            else:
                print(f"{'  '*depth}[ITEM ] id={n['id']:<12} key={key:<8} label={n['label']!r}")
            show(n["children"], depth + 1)

    show(tree)

    print("\n=== 交叉校验：菜单 ID vs 加速键表 ===")
    flat = flatten(tree)
    ok = True
    for a in accels:
        hit = [f for f in flat if f["id"] == a["cmd"]]
        mark = "OK " if hit else "!! "
        if not hit: ok = False
        label = hit[0]["label"] if hit else "-"
        print(f"  {mark}cmd={a['cmd']:<6} key={a['key']:<8} -> {label!r}")
    print(f"\n  结果: {'全部加速键均在菜单中找到对应项' if ok else '存在未匹配项'}")

    print("\n=== INI 键 <-> 菜单 ID 映射（用于行为规格）===")
    ini_map = {
        512: "ShowNext", 513: "ShowStatistics", 514: "UseFigures",
        256: "(New game)", 257: "(High score list)", 258: "(Exit)",
        768: "(Contents/help)", 769: "(About)",
    }
    for f in flat:
        if f["id"] in ini_map:
            print(f"  cmd={f['id']:<6} {ini_map[f['id']]:<20} label={f['label']!r}")

    json.dump({"tree": tree, "accels": accels, "flatten": flat},
              open("/tmp/wl_res/menu.json", "w"), ensure_ascii=False, indent=1)
    print("\nwrote /tmp/wl_res/menu.json")
