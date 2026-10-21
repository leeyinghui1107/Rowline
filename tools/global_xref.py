#!/usr/bin/env python3
"""
Locate every code site that references the 9x9 board array (VA 0x40B2F0)
and the other key globals; group hits by enclosing function.
"""
import struct, re, sys
sys.path.insert(0, "tools")
from disasm import PE
from capstone import Cs, CS_ARCH_X86, CS_MODE_32

pe = PE()
BOARD = 0x40B2F0
GLOBALS = {
    0x40B2F0: "board[81] (9x9 cells)",
    0x40B060: "boardOriginX", 0x40B064: "boardOriginY",
    0x40B068: "panelSpriteIdx", 0x40B069: "panelBase",
    0x40B06A: "panelFlags", 0x40B06C: "currentScore",
    0x40B070: "boardAreaWidth", 0x40B074: "boardCellCount",
    0x40B078: "boardOriginX_calc", 0x40B07C: "connectThreshold(5)",
    0x40B080: "figuresModeFlag", 0x40B084: "menuCmdRoute",
    0x40B088: "panelHalfWidth", 0x40B08C: "boardOriginY_calc",
    0x40B090: "stateFlags0", 0x40B094: "stateFlags1",
    0x40B098: "panelOriginX", 0x40B09C: "panelOriginY",
    0x40B0A0: "panelWidth(125)", 0x40B0A4: "stateFlags2",
    0x40B0AC: "scoreTable[16] x 32B",
    0x40B0C8: "scoreTable[0].score",
    0x40B2A8: "scoreTable[0].score(alias)",
    0x40B2B0: "atlasOldBitmap", 0x40B2B4: "atlasDC",
    0x40B2B8: "statusHwnd", 0x40B2BC: "cheatFlag",
    0x40B2C0: "mainHwnd", 0x40B2C8: "score2",
    0x40B3EC: "spriteSetToggle(0=sphere,1=poly)",
    0x40B3F0: "wipeAcceptedFlag",
    0x40B048: "hInstance",
    0x40B010: "rngState",
}

# function starts = internal call targets + known anchors
md = Cs(CS_ARCH_X86, CS_MODE_32)
md.detail = True
code = pe.raw(0x401000, 0x7400)
funcs = set()
for ins in md.disasm(code, 0x401000):
    if ins.mnemonic == "call" and ins.op_str.startswith("0x"):
        t = int(ins.op_str, 16)
        if 0x401000 <= t < 0x408400: funcs.add(t)
for a in (0x401000, 0x401133, 0x402DA2, 0x401533, 0x402A05, 0x402A50, 0x401698, 0x401EC6):
    funcs.add(a)
funcs = sorted(funcs)

def owner(addr):
    prev = None
    for f in funcs:
        if f <= addr: prev = f
        else: break
    return prev

hits = {}
for ins in md.disasm(code, 0x401000):
    m = re.search(r"0x40b[0-9a-f]{3}", ins.op_str)
    if not m: continue
    g = int(m.group(0), 16)
    if g in GLOBALS:
        hits.setdefault(owner(ins.address), {}).setdefault(g, []).append(ins.address)

print("=== 全局变量读写点（按所属函数分组）===\n")
for f in sorted(hits):
    name = GLOBALS.get(f, "")
    print(f"  func 0x{f:06X}  {name}")
    for g in sorted(hits[f]):
        locs = ", ".join(f"0x{a:06X}" for a in hits[f][g][:8])
        print(f"      {GLOBALS.get(g,'?'):<28} x{len(hits[f][g]):<3} @ {locs}")
    print()

print("=== 棋盘数组 0x40B2F0 的所有访问点 ===")
allb = sorted({a for f in hits for g in hits[f] if g == 0x40B2F0 for a in hits[f][g]})
print("  " + ", ".join(f"0x{a:06X}" for a in allb))
print(f"  -> 分布在 {len({owner(a) for a in allb})} 个函数中")
