#!/usr/bin/env python3
"""Find code xrefs to .data strings in the AUTO section (raw VA operands)."""
import struct, re, sys

d = open("/Users/thomas/Downloads/WINLINEZ.EXE", "rb").read()
CODE_FILE = 0x400
CODE_SIZE = 0x7400
DATA_VA = 0x40A000

def va2off(va):
    if 0x401000 <= va < 0x409000: return CODE_FILE + (va - 0x401000)
    if 0x40A000 <= va < 0x40B000: return 0x8600 + (va - 0x40A000)
    if 0x40B000 <= va < 0x40C000: return None
    return None

# collect .data strings
strings = {}
for m in re.finditer(rb"[\x20-\x7e]{4,}", d[0x8600:0x8C00]):
    s = m.group().decode()
    va = DATA_VA + m.start()
    strings[va] = s

code = d[CODE_FILE:CODE_FILE+CODE_SIZE]
print("=== xrefs from code to .data strings ===")
hits = {}
for va, s in strings.items():
    needle = struct.pack("<I", va)
    idx = 0
    while True:
        idx = code.find(needle, idx)
        if idx < 0: break
        cva = 0x401000 + idx
        hits.setdefault(s, []).append(cva)
        idx += 1

for va in sorted(strings):
    s = strings[va]
    xr = hits.get(s, [])
    print(f"  0x{va:06X}  {s[:60]:<62} xrefs: {['0x%06X'%x for x in xr]}")

# also the resource-id constants and WinMain/wndproc
print("\n=== key function addresses (from disasm landmarks) ===")
for name, va in [("WinMain", 0x401133), ("WndProc?", 0x402DA2),
                 ("helper_A", 0x405253), ("helper_B", 0x4052B5),
                 ("entry_stub", 0x401000)]:
    print(f"   {name:<12} VA=0x{va:06X}")

# how many distinct call targets (rough function count)
calls = set()
for m in re.finditer(rb"\xe8(....)", code):
    off = struct.unpack("<i", m.group(1))[0]
    tgt = 0x401000 + m.start() + 5 + off
    calls.add(tgt)
print(f"\n=== distinct direct call targets in AUTO section: {len(calls)} (rough function count) ===")
print("   lowest 10:", ["0x%06X"%c for c in sorted(calls)[:10]])
print("   highest 10:", ["0x%06X"%c for c in sorted(calls)[-10:]])
