#!/usr/bin/env python3
"""
Annotated disassembler for WINLINEZ.EXE.
Resolves IAT slots to Win32 API names and .data string addresses to literals.
Output is behavioural/structural analysis only - no code is copied.
"""
import struct, re, sys, json, os
from capstone import Cs, CS_ARCH_X86, CS_MODE_32

EXE = "/Users/thomas/Downloads/WINLINEZ.EXE"
BASE = 0x400000

class PE:
    def __init__(self, path=EXE):
        self.b = open(path, "rb").read()
        b = self.b
        self.e = struct.unpack_from("<I", b, 0x3C)[0]
        coff = self.e + 4
        self.nsec = struct.unpack_from("<H", b, coff + 2)[0]
        self.opt = coff + 20
        self.optsz = struct.unpack_from("<H", b, coff + 16)[0]
        self.entry = struct.unpack_from("<I", b, self.opt + 16)[0]
        self.segs = []
        so = self.opt + self.optsz
        for i in range(self.nsec):
            o = so + i*40
            nm = b[o:o+8].rstrip(b"\x00").decode("latin1")
            vs, va, rs, rp = struct.unpack_from("<IIII", b, o+8)
            self.segs.append((nm, va, vs, rp, rs))
        self.iat = self._iat()
        self.strs = self._strs()

    def off(self, rva):
        for nm, va, vs, rp, rs in self.segs:
            if va <= rva < va + max(vs, rs): return rp + (rva - va)
        return None

    def _cstr(self, o):
        return self.b[o:self.b.find(b"\x00", o)].decode("latin1")

    def _iat(self):
        dd = self.opt + 96
        imp_rva = struct.unpack_from("<I", self.b, dd + 8)[0]
        out = {}
        o = self.off(imp_rva)
        while True:
            ilt, ts, fc, nr, iatr = struct.unpack_from("<IIIII", self.b, o)
            if not (ilt or nr): break
            dll = self._cstr(self.off(nr))
            to = self.off(ilt or iatr)
            i = 0
            while True:
                v = struct.unpack_from("<I", self.b, to + i*4)[0]
                if not v: break
                nm = ("#%d" % (v & 0xffff)) if (v & 0x80000000) else self._cstr(self.off(v) + 2)
                out[BASE + iatr + i*4] = f"{dll}!{nm}"
                i += 1
            o += 20
        return out

    def _strs(self):
        out = {}
        for m in re.finditer(rb"[\x20-\x7e]{4,}", self.b[0x8600:0x8C00]):
            out[BASE + 0xA000 + m.start()] = m.group().decode("latin1")
        for m in re.finditer(rb"[\x20-\x7e]{8,}", self.b[0x400:0x7800]):
            s = m.group().decode("latin1")
            if " " in s and re.search(r"[A-Za-z]{3}", s):
                out[BASE + 0x1000 + m.start()] = s
        return out

    def raw(self, va, n):
        o = self.off(va - BASE)
        return self.b[o:o+n]

def disasm(pe, va, size, labels=None):
    md = Cs(CS_ARCH_X86, CS_MODE_32)
    md.detail = True
    code = pe.raw(va, size)
    out = []
    for ins in md.disasm(code, va):
        ann = ""
        op = ins.op_str
        # IAT call/jmp
        m = re.search(r"\[0x([0-9a-f]+)\]", op)
        if m and ins.mnemonic in ("call", "jmp"):
            a = int(m.group(1), 16)
            if a in pe.iat:
                ann = "  ; " + pe.iat[a]
        if not ann:
            m = re.search(r"(?:0x)?([0-9a-f]{6})\b", op)
            if m:
                a = int(m.group(1), 16)
                if a in pe.strs:
                    s = pe.strs[a].replace("\n", " ")
                    ann = f'  ; "{s[:44]}"'
                elif labels and a in labels:
                    ann = "  ; " + labels[a]
        txt = f"  {ins.address:08X}: {ins.mnemonic:<8} {ins.op_str}{ann}"
        out.append(txt)
    return out

def find_calls(pe, api):
    """Find all call sites of a given Win32 API (by name fragment)."""
    res = []
    md = Cs(CS_ARCH_X86, CS_MODE_32)
    md.detail = True
    code = pe.raw(BASE + 0x1000, 0x7400)
    for ins in md.disasm(code, BASE + 0x1000):
        if ins.mnemonic not in ("call", "jmp"): continue
        m = re.search(r"\[0x([0-9a-f]+)\]", ins.op_str)
        if not m: continue
        a = int(m.group(1), 16)
        if a in pe.iat and api.lower() in pe.iat[a].lower():
            res.append(ins.address)
    return res

if __name__ == "__main__":
    pe = PE()
    cmd = sys.argv[1] if len(sys.argv) > 1 else "info"
    if cmd == "info":
        print(f"IAT slots: {len(pe.iat)}   .data/code strings: {len(pe.strs)}")
        for a in sorted(pe.iat): print(f"  0x{a:06X}  {pe.iat[a]}")
    elif cmd == "calls":
        for api in sys.argv[2:]:
            for a in find_calls(pe, api):
                print(f"  0x{a:06X}  call {api}")
    else:
        va = int(sys.argv[2], 16); size = int(sys.argv[3], 0) if len(sys.argv) > 3 else 0x100
        for l in disasm(pe, va, size): print(l)
