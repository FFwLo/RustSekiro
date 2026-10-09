"""Maps every vtable in the exe to its MSVC RTTI class name (x64 complete object locators)
and annotates extracted/decomp/*.c: `&PTR_FUN_142a73540` -> `&PTR_FUN_142a73540 /* CSChrAutoHomingModule */`.
Writes extracted/rtti_vtables.txt. Usage: python tools/rtti_vtables.py [exe] [decomp_dir]"""
import re, struct, sys, glob

exe = sys.argv[1] if len(sys.argv) > 1 else "extracted/sekiro_steamless.exe"
ddir = sys.argv[2] if len(sys.argv) > 2 else "extracted/decomp"
b = open(exe, "rb").read()
pe = struct.unpack_from("<I", b, 0x3C)[0]
n = struct.unpack_from("<H", b, pe + 6)[0]
opt = struct.unpack_from("<H", b, pe + 20)[0]
base = struct.unpack_from("<Q", b, pe + 24 + 24)[0]
secs = [struct.unpack_from("<8sIIII", b, pe + 24 + opt + 40 * i) for i in range(n)]

def off(rva):
    for _, vs, va, rs, rp in secs:
        if va <= rva < va + max(vs, rs) and rva - va < rs:
            return rva - va + rp
    return None

def rva_of(o):
    for _, vs, va, rs, rp in secs:
        if rp <= o < rp + rs:
            return o - rp + va
    return None

def demangle(s):
    # ".?AVName@NS@@" -> "NS::Name"
    m = re.match(r"\.\?A[VU](.+)@@$", s)
    return "::".join(reversed(m.group(1).split("@"))) if m else s

# Type descriptors: ".?AV...@@" strings at TD + 0x10.
tds = {}
for m in re.finditer(rb"\.\?A[VU][\x21-\x7e]+?@@\x00", b):
    tds[rva_of(m.start() - 0x10)] = demangle(m.group()[:-1].decode())
# COLs: signature 1, then pTypeDescriptor RVA at +0xC, pSelf RVA at +0x14 == own RVA.
cols = {}
for o in range(0, len(b) - 0x18, 4):
    if b[o] == 1 and b[o + 1 : o + 4] == b"\0\0\0":
        td = struct.unpack_from("<I", b, o + 12)[0]
        if td in tds:
            r = rva_of(o)
            if r is not None and struct.unpack_from("<I", b, o + 20)[0] == r:
                cols[base + r] = (tds[td], struct.unpack_from("<I", b, o + 4)[0])
# Vtables: a qword pointing at a COL; the vtable starts right after it.
vt = {}
colset = set(cols)
for o in range(0, len(b) - 8, 8):
    q = struct.unpack_from("<Q", b, o)[0]
    if q in colset:
        r = rva_of(o + 8)
        if r is not None:
            name, offset = cols[q]
            vt[base + r] = name + (f" (+{offset:#x})" if offset else "")
with open("extracted/rtti_vtables.txt", "w", encoding="utf-8") as f:
    for a in sorted(vt):
        f.write(f"{a:#x} {vt[a]}\n")
print(f"{len(tds)} type descriptors, {len(cols)} locators, {len(vt)} vtables")

pat = re.compile(r"(PTR_(?:FUN|LAB|DAT)_(14[0-9a-f]{7}))(?! /\*)")
total = 0
for path in glob.glob(f"{ddir}/*.c"):
    t = open(path, encoding="utf-8", errors="replace").read()
    def sub(m):
        name = vt.get(int(m.group(2), 16))
        return f"{m.group(1)} /* {name.split('::')[-1]} */" if name else m.group(1)
    t2, k = pat.subn(sub, t)
    if t2 != t:
        open(path, "w", encoding="utf-8").write(t2)
        total += t2.count("*/") - t.count("*/")
print(f"annotated {total} vtable references")
