"""Reads a memscope recording (memscope-data/logs/rec_*/frames.bin, written by recorder.lua).

  python tools/rec_read.py <rec_dir>                       summary: ticks, slots, blocks, positions
  python tools/rec_read.py <rec_dir> --field 0:physics:0x80:f3 [--every 10]
      one field over time: slot:block:offset:type (types: i32 u32 f32 f3 (3 floats) u8 u16 i16 u64 hex16)
  python tools/rec_read.py <rec_dir> --changes 0:timeact [--from 0 --to 300]
      offsets (4-byte words) of a block that change, with first/last values: finds the live fields

Records: "<I4 d B B I2" (tick, clock ms, slot, block id, length) + raw bytes. Block names: index.txt.
"""
import struct
import sys
from collections import defaultdict
from pathlib import Path

HDR = struct.Struct("<IdBBH")


def load(rec_dir):
    d = Path(rec_dir)
    names = {}
    for line in (d / "index.txt").read_text().splitlines():
        parts = line.split()
        if parts and parts[0].isdigit():
            names[int(parts[0])] = parts[1]
    by_name = {v: k for k, v in names.items()}
    data = (d / "frames.bin").read_bytes()
    recs = defaultdict(dict)  # (slot, block) -> {tick: (clock, bytes)}
    o = 0
    while o + HDR.size <= len(data):
        tick, clock, slot, block, n = HDR.unpack_from(data, o)
        o += HDR.size
        recs[(slot, block)][tick] = (clock, data[o:o + n])
        o += n
    return names, by_name, recs


FMT = {"i32": ("<i", 4), "u32": ("<I", 4), "f32": ("<f", 4), "f3": ("<3f", 12), "u8": ("<B", 1),
       "u16": ("<H", 2), "i16": ("<h", 2), "u64": ("<Q", 8)}


def value(raw, off, ty):
    if ty == "hex16":
        return raw[off:off + 16].hex(" ")
    fmt, n = FMT[ty]
    if off + n > len(raw):
        return None
    v = struct.unpack_from(fmt, raw, off)
    return v if len(v) > 1 else v[0]


def main():
    if len(sys.argv) < 2:
        print(__doc__)
        return
    names, by_name, recs = load(sys.argv[1])
    args = sys.argv[2:]
    opt = lambda k, d=None: args[args.index(k) + 1] if k in args else d
    if "--field" in args:
        slot, block, off, ty = opt("--field").split(":")
        rows = recs[(int(slot), by_name[block])]
        every = int(opt("--every", 1))
        for tick in sorted(rows)[::every]:
            clock, raw = rows[tick]
            print(f"{tick:6d} {clock / 1000:8.3f}s {value(raw, int(off, 0), ty)}")
        return
    if "--changes" in args:
        slot, block = opt("--changes").split(":")
        rows = recs[(int(slot), by_name[block])]
        lo, hi = int(opt("--from", 0)), int(opt("--to", 10**9))
        ticks = [t for t in sorted(rows) if lo <= t <= hi]
        if not ticks:
            return
        n = len(rows[ticks[0]][1])
        for off in range(0, n - 3, 4):
            vals = [struct.unpack_from("<I", rows[t][1], off)[0] for t in ticks]
            distinct = len(set(vals))
            if distinct > 1:
                f0, f1 = struct.unpack_from("<f", rows[ticks[0]][1], off)[0], struct.unpack_from("<f", rows[ticks[-1]][1], off)[0]
                print(f"+0x{off:03X} {distinct:4d} values  int {vals[0]} -> {vals[-1]}  float {f0:.4g} -> {f1:.4g}")
        return
    ticks = sorted({t for rows in recs.values() for t in rows})
    print(f"{len(ticks)} ticks, {ticks[0]}..{ticks[-1]}")
    for (slot, block), rows in sorted(recs.items()):
        print(f"slot {slot} {names.get(block, block):16s} {len(rows)} records, {len(next(iter(rows.values()))[1])} bytes")
    for slot in range(3):
        rows = recs.get((slot, by_name["physics"]))
        if rows:
            first, last = rows[min(rows)][1], rows[max(rows)][1]
            print(f"slot {slot} pos {value(first, 0x80, 'f3')} -> {value(last, 0x80, 'f3')}")
        data = recs.get((slot, by_name["data"]))
        if data:
            raw = data[max(data)][1]
            print(f"slot {slot} hp {value(raw, 0x30, 'i32')}/{value(raw, 0x34, 'i32')} posture words {[value(raw, o, 'i32') for o in (0x48, 0x4C, 0x50)]}")


if __name__ == "__main__":
    main()


def body_anims(rows):
    """tick -> current body anim from TimeAct +0x100, skipping the facial layer (a000_7900x0 =
    AddBlendFace_N CMSGs in c0000.hkx), which interleaves with the body anim in that field."""
    out, last = {}, None
    for t in sorted(rows):
        a = struct.unpack_from('<i', rows[t][1], 0x100)[0]
        if not (a < 1000000 and 790000 <= a < 800000):
            last = a
        out[t] = last if last is not None else a
    return out
