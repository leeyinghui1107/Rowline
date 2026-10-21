#!/usr/bin/env python3
"""Walk the PE resource tree and dump every leaf entry to disk."""
import struct, sys, os, collections, math

def u16(b,o): return struct.unpack_from("<H",b,o)[0]
def u32(b,o): return struct.unpack_from("<I",b,o)[0]
def u64(b,o): return struct.unpack_from("<Q",b,o)[0]

TYPES = {1:"CURSOR",2:"BITMAP",3:"ICON",4:"MENU",5:"DIALOG",6:"STRING",7:"FONTDIR",8:"FONT",
         9:"ACCELERATOR",10:"RCDATA",11:"MESSAGETABLE",12:"GROUP_CURSOR",14:"GROUP_ICON",
         16:"VERSION",17:"DLGINCLUDE",19:"PLUGPLAY",20:"VXD",21:"ANICURSOR",22:"ANIICON",
         23:"HTML",24:"MANIFEST"}

class PE:
    def __init__(self, path):
        self.b = open(path,"rb").read()
        b = self.b
        e = u32(b,0x3C); coff = e+4
        nsec = u16(b,coff+2); opt = coff+20; opt_size = u16(b,coff+16)
        magic = u16(b,opt); self.p64 = magic==0x20b
        dd = opt + (112 if self.p64 else 96)
        self.rsrc_rva, self.rsrc_size = u32(b,dd+2*8), u32(b,dd+2*8+4)
        self.segs=[]
        so = opt+opt_size
        for i in range(nsec):
            o = so+i*40
            self.segs.append((u32(b,o+12), u32(b,o+8), u32(b,o+20), u32(b,o+16)))  # va,vsize,raw,rawsize
    def off(self, rva):
        for va,vs,raw,rs in self.segs:
            if va <= rva < va+max(vs,rs):
                return raw + (rva-va)
        return None

def name_of(b, o, is_name):
    if is_name:
        ln = u16(b,o); return b[o+2:o+2+ln*2].decode("utf-16-le","replace")
    return str(u32(b,o))

def walk(pe, base_off, off, depth, path, out):
    b = pe.b
    named = u16(b,off+12); ids = u16(b,off+14)
    for i in range(named+ids):
        eo = off+16+i*8
        nid, doff = u32(b,eo), u32(b,eo+4)
        if nid & 0x80000000:
            # name string RVA is relative to the START OF THE RESOURCE SECTION
            label = name_of(b, base_off+(nid&0x7fffffff), True)
        else:
            label = str(nid)
        if depth==0:
            label = TYPES.get(int(label) if label.isdigit() else -1, f"TYPE_{label}")
        if doff & 0x80000000:
            walk(pe, base_off, base_off+(doff&0x7fffffff), depth+1, path+[label], out)
        else:
            de = base_off+doff
            drva, dsz, cp, _ = struct.unpack_from("<IIII", b, de)
            out.append((path+[label], drva, dsz, cp))

if __name__=="__main__":
    pe = PE(sys.argv[1])
    outdir = sys.argv[2] if len(sys.argv)>2 else "/tmp/wl_res"
    os.makedirs(outdir, exist_ok=True)
    base = pe.off(pe.rsrc_rva)
    out=[]
    walk(pe, base, base, 0, [], out)
    print(f"resource dir RVA=0x{pe.rsrc_rva:X} base_off=0x{base:X}  leaves={len(out)}")
    total=0
    manifest=[]
    for path, drva, dsz, cp in sorted(out, key=lambda x:-x[2]):
        total+=dsz
        p = "/".join(path)
        print(f"  {p:<48} size={dsz:>9} ({dsz/1024:8.1f} KB)  codepage={cp}")
        manifest.append((path,drva,dsz,cp))
    print(f"TOTAL resource payload: {total} bytes ({total/1024:.1f} KB)")
    # extract
    import json
    json.dump([{"path":p,"rva":r,"size":s} for p,r,s,c in manifest], open(os.path.join(outdir,"_manifest.json"),"w"), indent=1)
    for path, drva, dsz, cp in manifest:
        o = pe.off(drva)
        if o is None: continue
        data = pe.b[o:o+dsz]
        safe = "_".join(str(x) for x in path).replace("/","__")
        ext = os.path.splitext(safe)[1]
        open(os.path.join(outdir, safe), "wb").write(data)
    print(f"extracted to {outdir}")
