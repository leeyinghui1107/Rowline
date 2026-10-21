#!/usr/bin/env python3
"""Compact structural summary of a function: constants, compares, calls, memory refs."""
import sys, re
sys.path.insert(0, "tools")
from disasm import PE
from capstone import Cs, CS_ARCH_X86, CS_MODE_32

pe = PE()
md = Cs(CS_ARCH_X86, CS_MODE_32)

def summarize(va, size, title=""):
    print(f"\n{'='*72}\n### func 0x{va:06X}  {title}\n{'='*72}")
    calls, consts, cmps, refs = [], [], [], []
    for ins in md.disasm(pe.raw(va, size), va):
        m = re.search(r"\[0x([0-9a-f]+)\]", ins.op_str)
        if m:
            a = int(m.group(1), 16)
            if a in pe.iat: calls.append((ins.address, pe.iat[a]))
            elif 0x40a000 <= a < 0x40c000: refs.append((ins.address, a))
        if ins.mnemonic == "call" and ins.op_str.startswith("0x"):
            calls.append((ins.address, f"sub_{ins.op_str}"))
        if ins.mnemonic == "cmp":
            m2 = re.search(r",\s*(-?0x[0-9a-f]+|\d+)$", ins.op_str)
            if m2:
                v = int(m2.group(1), 0)
                if v > 0x100: consts.append((ins.address, v))
        if ins.mnemonic in ("push", "mov") and re.search(r",\s*0x[0-9a-f]+$", ins.op_str):
            v = int(re.search(r"0x([0-9a-f]+)$", ins.op_str).group(1), 16)
            if 0x40 <= v <= 0xFFFF and v not in (0x40, 0x80): consts.append((ins.address, v))
    seen = set()
    print("-- Win32/internal calls --")
    for a, n in calls:
        k = (a, n)
        if k in seen: continue
        seen.add(k)
        print(f"   0x{a:06X}  {n}")
    print("-- key constants --")
    seen = set()
    for a, v in consts:
        if (v, a) in seen: continue
        seen.add((v, a))
        print(f"   0x{a:06X}  0x{v:X} ({v})")
    print("-- global refs --")
    seen = set()
    for a, g in refs:
        if (g, a) in seen: continue
        seen.add((g, a))
        print(f"   0x{a:06X}  0x{g:06X}")

if __name__ == "__main__":
    for spec in sys.argv[1:]:
        va, size, title = spec.split(":", 2)
        summarize(int(va, 16), int(size, 0), title)
