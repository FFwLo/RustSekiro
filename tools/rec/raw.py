"""Streaming reader for frames.bin (rec2.lua and the old recorder.lua share the record format)."""
import json
import mmap
import struct
from pathlib import Path

from .fields import TYPES

HDR = struct.Struct("<IdBBH")


def block_names(rec):
    """block id -> name, from manifest_lua.json (rec2) or index.txt (recorder.lua)."""
    rec = Path(rec)
    m = rec / "manifest_lua.json"
    if m.exists():
        return {int(k): v["name"] for k, v in json.loads(m.read_text())["blocks"].items()}
    names = {}
    idx = rec / "index.txt"
    if idx.exists():
        for line in idx.read_text().splitlines():
            p = line.split()
            if p and p[0].isdigit():
                names[int(p[0])] = p[1]
    return names


def records(rec):
    """Yields (tick, ms, slot, block id, bytes) in file order."""
    path = Path(rec) / "frames.bin"
    if not path.exists() or path.stat().st_size == 0:
        return
    with open(path, "rb") as fh, mmap.mmap(fh.fileno(), 0, access=mmap.ACCESS_READ) as mm:
        o, n = 0, len(mm)
        while o + HDR.size <= n:
            tick, ms, slot, block, ln = HDR.unpack_from(mm, o)
            o += HDR.size
            if o + ln > n:
                break
            yield tick, ms, slot, block, mm[o:o + ln]
            o += ln


def value(raw, off, ty):
    fmt, n = TYPES[ty]
    if raw is None or off + n > len(raw):
        return None
    return struct.unpack_from(fmt, raw, off)[0]


def blocks_of(rec, slot, block_name, lo=0, hi=10**9):
    """{tick: bytes} for one slot and block (for drill-down; loads only that stream)."""
    ids = {v: k for k, v in block_names(rec).items()}
    bid = ids[block_name]
    return {t: b for t, _, s, k, b in records(rec) if s == slot and k == bid and lo <= t <= hi}
