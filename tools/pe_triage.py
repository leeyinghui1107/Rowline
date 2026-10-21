#!/usr/bin/env python3
"""Minimal dependency-free PE triage: headers, sections, data dirs, imports, resources, overlay."""
import struct, sys, datetime

DATA_DIRS = [
    "EXPORT", "IMPORT", "RESOURCE", "EXCEPTION", "SECURITY", "BASERELOC",
    "DEBUG", "ARCHITECTURE", "GLOBALPTR", "TLS", "LOAD_CONFIG", "BOUND_IMPORT",
    "IAT", "DELAY_IMPORT", "CLR_RUNTIME", "RESERVED",
]

def u16(b, o): return struct.unpack_from("<H", b, o)[0]
def u32(b, o): return struct.unpack_from("<I", b, o)[0]
def u64(b, o): return struct.unpack_from("<Q", b, o)[0]

def rva2off(rva, secs):
    for s in secs:
        if s["va"] <= rva < s["va"] + max(s["vsize"], s["rawsize"]):
            return s["raw"] + (rva - s["va"])
    return None

def cstr(b, o):
    e = b.find(b"\x00", o)
    return b[o:e].decode("latin-1", "replace")

def main(path):
    b = open(path, "rb").read()
    print(f"== FILE: {path}")
    print(f"   size={len(b)} bytes ({len(b)/1024:.1f} KB)")

    e_lfanew = u32(b, 0x3C)
    if b[e_lfanew:e_lfanew+4] != b"PE\x00\x00":
        print("!! not a PE"); return
    coff = e_lfanew + 4
    machine = u16(b, coff)
    nsec = u16(b, coff+2)
    timestamp = u32(b, coff+4)
    opt_size = u16(b, coff+16)
    chars = u16(b, coff+18)
    print(f"== COFF header @0x{e_lfanew:X}")
    print(f"   machine=0x{machine:04X} ({'i386' if machine==0x14c else 'x64' if machine==0x8664 else 'other'})")
    print(f"   sections={nsec}  timestamp=0x{timestamp:08X} ({datetime.datetime.utcfromtimestamp(timestamp).isoformat()}Z)")
    print(f"   characteristics=0x{chars:04X} " + " ".join(
        n for n, v in [("EXECUTABLE",0x0002),("NO_RELOC",0x0001),("32BIT",0x0100),
                       ("DLL",0x2000),("UPOS_ONLY",0x200),("STRIPPED",0x0008)] if chars & v))

    opt = coff + 20
    magic = u16(b, opt)
    pe32plus = magic == 0x20b
    print(f"== OPTIONAL header  magic=0x{magic:04X} ({'PE32+' if pe32plus else 'PE32'})")
    linker_maj, linker_min = b[opt+2], b[opt+3]
    print(f"   linker version={linker_maj}.{linker_min}")
    entry_rva = u32(b, opt+16)
    image_base = u64(b, opt+24) if pe32plus else u32(b, opt+28)
    sect_align = u32(b, opt+32)
    file_align = u32(b, opt+36)
    subsystem = u16(b, opt+68)
    dllchars = u16(b, opt+70)
    nrva = u32(b, opt + (108 if pe32plus else 92))
    print(f"   entrypoint RVA=0x{entry_rva:X}  imagebase=0x{image_base:X}  EP VA=0x{image_base+entry_rva:X}")
    print(f"   subsystem={subsystem} ({'GUI' if subsystem==2 else 'CUI' if subsystem==3 else 'other'})  dllchars=0x{dllchars:04X} " +
          " ".join(n for n, v in [("DYNAMIC_BASE(ASLR)",0x0040),("NX_COMPAT(DEP)",0x0100),
                                  ("NO_SEH",0x0400),("TERMINAL_SERVER_AWARE",0x8000)] if dllchars & v))
    print(f"   section_align=0x{sect_align:X} file_align=0x{file_align:X} datadirs={nrva}")

    print("== DATA DIRECTORIES")
    dd_off = opt + (112 if pe32plus else 96)
    dirs = {}
    for i in range(min(nrva, 16)):
        rva, size = u32(b, dd_off + i*8), u32(b, dd_off + i*8 + 4)
        dirs[DATA_DIRS[i] if i < len(DATA_DIRS) else f"DIR{i}"] = (rva, size)
        if size:
            print(f"   {DATA_DIRS[i]:<14} RVA=0x{rva:08X} size=0x{size:X} ({size})")

    sec_off = opt + opt_size
    secs = []
    print("== SECTIONS")
    for i in range(nsec):
        o = sec_off + i*40
        name = b[o:o+8].rstrip(b"\x00").decode("latin-1")
        vsize, va, rsize, raw = u32(b,o+8), u32(b,o+12), u32(b,o+16), u32(b,o+20)
        sc = u32(b, o+36)
        secs.append(dict(name=name, va=va, vsize=vsize, raw=raw, rawsize=rsize))
        flags = " ".join(n for n, v in [("CODE",0x20),("IDATA",0x40),("UDATA",0x80),("DISCARD",0x02000000),
                                        ("SHARED",0x10000000),("EXEC",0x20000000),("READ",0x40000000),("WRITE",0x80000000)] if sc & v)
        entropy = 0
        if rsize:
            import math, collections
            chunk = b[raw:raw+rsize]
            c = collections.Counter(chunk)
            n = len(chunk)
            entropy = -sum((v/n)*math.log2(v/n) for v in c.values())
        print(f"   {name:<9} VA=0x{va:08X} VSize=0x{vsize:07X} Raw=0x{raw:07X} RawSz=0x{rsize:07X} "
              f"{'E' if sc&0x20000000 else '-'}{'R' if sc&0x40000000 else '-'}{'W' if sc&0x80000000 else '-'} "
              f"entropy={entropy:.2f}  {flags}")
        if entropy > 7.2:
            print(f"        ^^ HIGH ENTROPY: compressed/encrypted/packed content")

    # Overlay
    last = max((s["raw"]+s["rawsize"]) for s in secs if s["rawsize"])
    if last < len(b):
        print(f"== OVERLAY: {len(b)-last} bytes at 0x{last:X} ({(len(b)-last)/1024:.1f} KB)")
    else:
        print("== OVERLAY: none")

    # Imports
    print("== IMPORTS")
    if "IMPORT" in dirs and dirs["IMPORT"][1]:
        imp_rva = dirs["IMPORT"][0]
        o = rva2off(imp_rva, secs)
        if o:
            while True:
                ilt_rva, ts, fchain, name_rva, iat_rva = struct.unpack_from("<IIIII", b, o)
                if name_rva == 0 and ilt_rva == 0: break
                dll = cstr(b, rva2off(name_rva, secs) or 0)
                funcs = []
                thunk = ilt_rva or iat_rva
                to = rva2off(thunk, secs)
                if to:
                    step = 8 if pe32plus else 4
                    for k in range(400):
                        v = u64(b, to+k*step) if pe32plus else u32(b, to+k*step)
                        if v == 0: break
                        if v & (1 << (63 if pe32plus else 31)):
                            funcs.append("#%d" % (v & 0xffff))
                        else:
                            hn = rva2off(v & 0x7fffffff, secs)
                            funcs.append(cstr(b, hn+2) if hn else "?")
                print(f"   {dll}  ({len(funcs)} funcs)")
                print(f"      {', '.join(funcs)}")
                o += 20

    # CLR
    if "CLR_RUNTIME" in dirs and dirs["CLR_RUNTIME"][1]:
        print("!! .NET (CLR) runtime header PRESENT -> managed assembly")

    # Resources
    if "RESOURCE" in dirs and dirs["RESOURCE"][1]:
        print("== RESOURCES")
        try:
            def walk(rva, depth=0, path="R"):
                off = rva2off(rva, secs)
                if off is None: return
                named = u16(b, off+12); ids = u16(b, off+14)
                for i in range(named+ids):
                    eo = off + 16 + i*8
                    nameid, dataoff = u32(b, eo), u32(b, eo+4)
                    if depth == 0:
                        types = {1:"CURSOR",2:"BITMAP",3:"ICON",4:"MENU",5:"DIALOG",6:"STRING",7:"FONTDIR",
                                 8:"FONT",9:"ACCELERATOR",10:"RCDATA",11:"MESSAGETABLE",12:"GROUP_CURSOR",
                                 14:"GROUP_ICON",16:"VERSION",24:"MANIFEST"}
                        label = types.get(nameid, f"TYPE_{nameid}")
                    else:
                        label = str(nameid)
                    if dataoff & 0x80000000:
                        walk((dataoff & 0x7fffffff), depth+1, path+"/"+label)
                    else:
                        do = rva2off(dataoff, secs)
                        if do is not None:
                            drva, dsz, cp, _ = struct.unpack_from("<IIII", b, do)
                            print(f"   {path}/{label}  RVA=0x{drva:X} size={dsz} ({dsz/1024:.1f}KB) cp={cp}")
            walk(dirs["RESOURCE"][0])
        except Exception as ex:
            print("   resource parse error:", ex)

    # VERSION info
    print("== VERSION / METADATA scan")
    for pat, label in [(b"VS_VERSION_INFO","VS_VERSION_INFO"), (b"FileDescription","FileDescription"),
                       (b"CompanyName","CompanyName"), (b"ProductName","ProductName"),
                       (b"OriginalFilename","OriginalFilename"), (b"InternalName","InternalName"),
                       (b"LegalCopyright","LegalCopyright"), (b"FileVersion","FileVersion")]:
        idx = b.find(pat)
        if idx >= 0:
            # try to read the following UTF-16 value
            seg = b[idx:idx+400]
            try:
                txt = seg.decode("utf-16-le", "ignore")
                m = [x for x in txt.split("\x00") if x.isprintable() and len(x) > 2]
                print(f"   {label}: {m[:3]}")
            except Exception:
                print(f"   {label}: present")
        else:
            print(f"   {label}: -")

if __name__ == "__main__":
    main(sys.argv[1] if len(sys.argv) > 1 else "/Users/thomas/Downloads/WINLINEZ.EXE")
