#!/usr/bin/env python3
"""
Command-dispatch extractor for WINLINEZ.EXE.

Because the Borland menu uses command IDs 0x100-0x102 / 0x200-0x202 / 0x300-0x301,
they numerically collide with Windows message constants (WM_KEYDOWN, WM_MOUSEMOVE,
WM_APP ...).  Disambiguation rule used here:
    * a comparison against `msg`  -> window message
    * a comparison against LOWORD(wParam) inside the WM_COMMAND handler -> menu ID
The two dispatch trees are located separately and cross-validated against
RT_ACCELERATOR.
"""
import sys, re, json
sys.path.insert(0, "tools")
from disasm import PE
from capstone import Cs, CS_ARCH_X86, CS_MODE_32

pe = PE()
md = Cs(CS_ARCH_X86, CS_MODE_32)

MENU = json.load(open("/tmp/wl_res/menu.json", encoding="utf-8"))
ACC = {a["cmd"]: a["key"] for a in MENU["accels"]}
LABEL = {f["id"]: f["label"] for f in MENU["flatten"] if f["id"]}

WM = {0x24: "WM_GETFONT", 0x111: "WM_COMMAND", 0x201: "WM_MOUSEMOVE",
      0x202: "WM_LBUTTONDOWN", 0x203: "WM_LBUTTONUP", 0x111: "WM_COMMAND",
      0x900: "WM_APP_USER", 0x00F: "WM_PAINT", 0x113: "WM_TIMER",
      0x02: "WM_DESTROY", 0x10: "WM_CLOSE", 0x100: "WM_KEYDOWN"}

# The two dispatch trees (located by hand, verified by the cross-check below)
MSG_TREE = 0x403B65          # compares `edi` = msg
CMD_TREE = 0x40366D          # compares `ax`  = LOWORD(wParam)

def tree_edges(start, limit):
    """Return the comparison constants used inside a dispatch tree."""
    out = []
    for ins in md.disasm(pe.raw(start, limit), start):
        m = re.search(r"^(\w+),\s*(0x[0-9a-f]+|\d+)\b", ins.op_str)
        if m:
            out.append((ins.address, m.group(1), int(m.group(2), 0)))
    return out

def find_targets(start, limit):
    """Collect direct branch targets inside the tree."""
    t = set()
    for ins in md.disasm(pe.raw(start, limit), start):
        if ins.mnemonic.startswith("j") and ins.op_str.startswith("0x"):
            t.add(int(ins.op_str, 16))
    return sorted(t)

if __name__ == "__main__":
    print("=== 窗口消息分派树 @ 0x%06X（比较 msg）===" % MSG_TREE)
    for a, reg, v in tree_edges(MSG_TREE, 0x90):
        print(f"   0x{a:06X}  cmp {reg}, 0x{v:X}   -> {WM.get(v, '?')}")
    print("   分支目标:", ", ".join("0x%06X" % t for t in find_targets(MSG_TREE, 0x90)))

    print("\n=== WM_COMMAND 命令分派树 @ 0x%06X（比较 LOWORD(wParam)）===" % CMD_TREE)
    for a, reg, v in tree_edges(CMD_TREE, 0x60):
        key = ACC.get(v, "")
        lab = LABEL.get(v, "")
        print(f"   0x{a:06X}  cmp {reg}, 0x{v:X}   -> cmd {v:<6} key={key:<8} {lab!r}")
    print("   分支目标:", ", ".join("0x%06X" % t for t in find_targets(CMD_TREE, 0x60)))

    print("\n=== 菜单 ID ↔ INI 键 交叉验证 ===")
    INI = {512: "ShowNext", 513: "ShowStatistics", 514: "UseFigures"}
    for cmd, ini in INI.items():
        hit = [f for f in MENU["flatten"] if f["id"] == cmd]
        lab = hit[0]["label"] if hit else "?"
        print(f"   cmd={cmd:<5} INI {ini:<18} key={ACC.get(cmd,'-'):<6} label={lab!r}")
