"""Drill-down queries with capped output (default 40 rows). Presets or raw SQL (SELECT only)."""
import sqlite3
import struct

from . import raw
from .fields import TYPES
from .ingest import ingest

PRESETS = {
    "anims": ("SELECT slot, who, name, start_tick, round(dur_ms) ms, round(speed, 2) mps, round(turn_deg) turn, prev, next "
              "FROM anims WHERE (? IS NULL OR slot = ?) ORDER BY start_tick", 1),
    "anim": ("SELECT slot, who, name, start_tick, round(dur_ms) ms, round(speed, 2) mps, round(turn_deg) turn, prev, next "
             "FROM anims WHERE name = ? ORDER BY start_tick", 1),
    "hits": ("SELECT tick, victim, hp_delta hp, posture_delta po, victim_anim, reaction, attacker_anim, round(attacker_anim_ms) at_ms, "
             "round(dist, 2) dist FROM hits WHERE (? IS NULL OR victim_slot = ?) ORDER BY tick", 1),
    "events": ("SELECT tick, round(ms) ms, slot, kind, a, b, text FROM events WHERE (? IS NULL OR kind = ?) ORDER BY tick", 1),
    "speffects": ("SELECT slot, who, id, start_tick, end_tick, round(dur_ms) ms, anim FROM speffects "
                  "WHERE (? IS NULL OR id = ?) ORDER BY start_tick", 1),
    "acts": ("SELECT tick, who, a act, b prev FROM events WHERE kind IN ('ai_act', 'ai_kengeki') AND (? IS NULL OR slot = ?) ORDER BY tick", 1),
    "marks": ("SELECT tick, round(ms) ms, kind, text FROM events WHERE kind IN ('mark', 'cmd', 'hook', 'slot') "
              "AND (? IS NULL OR 1 = ?) ORDER BY tick", 1),
    "hooks": ("SELECT tick, name, a1, a2, a3 FROM hooks WHERE (? IS NULL OR name = ?) ORDER BY tick", 1),
    "frames": ("SELECT tick, slot, anim, round(x, 2) x, round(y, 2) y, round(z, 2) z, round(yaw, 3) yaw, round(speed, 2) mps, "
               "hp, posture, ai_act FROM frames WHERE tick BETWEEN ? AND ? + 120 ORDER BY tick, slot", 2),
}

HELP = """presets: anims [slot] | anim <name> | hits [victim slot] | events [kind] | speffects [id] | acts [slot]
         | marks | hooks [name] | frames <tick> (2 s window) | field <slot:block:off:type> [from to every]
         | changes <slot:block> [from to] | sql "<select ...>"  (types: i32 u32 f32 u8 u16 i16 u64)"""


def _num(v):
    try:
        return int(v, 0)
    except (TypeError, ValueError):
        return v


def run(rec, what, args, limit=40):
    if what == "field":
        slot, block, off, ty = args[0].split(":")
        lo, hi = (int(args[1]), int(args[2])) if len(args) >= 3 else (0, 10**9)
        every = int(args[3]) if len(args) >= 4 else 1
        rows = raw.blocks_of(rec, int(slot), block, lo, hi)
        ticks = sorted(rows)[::every]
        out = [f"{t} {raw.value(rows[t], int(off, 0), ty)}" for t in ticks[:limit]]
        return out + ([f"... {len(ticks) - limit} more"] if len(ticks) > limit else [])
    if what == "changes":
        slot, block = args[0].split(":")
        lo, hi = (int(args[1]), int(args[2])) if len(args) >= 3 else (0, 10**9)
        rows = raw.blocks_of(rec, int(slot), block, lo, hi)
        ticks = sorted(rows)
        if not ticks:
            return ["no data"]
        n, out = len(rows[ticks[0]]), []
        for off in range(0, n - 3, 4):
            vals = [struct.unpack_from("<I", rows[t], off)[0] for t in ticks if len(rows[t]) >= off + 4]
            d = len(set(vals))
            if d > 1:
                f0 = struct.unpack_from("<f", rows[ticks[0]], off)[0]
                f1 = struct.unpack_from("<f", rows[ticks[-1]], off)[0]
                out.append(f"+0x{off:03X} {d:5d} values  int {vals[0]} -> {vals[-1]}  float {f0:.4g} -> {f1:.4g}")
        return out[:limit] + ([f"... {len(out) - limit} more"] if len(out) > limit else [])
    db = sqlite3.connect(ingest(rec))
    if what == "sql":
        q = args[0]
        if not q.lstrip().lower().startswith(("select", "with")):
            return ["only SELECT queries"]
        cur = db.execute(q)
    elif what in PRESETS:
        q, n = PRESETS[what]
        a = [_num(x) for x in args[:1]] or [None]
        params = (a[0], a[0]) if n == 1 else (a[0], a[0])
        cur = db.execute(q, params)
    else:
        return [HELP]
    cols = [c[0] for c in cur.description]
    rows = cur.fetchmany(limit + 1)
    out = ["\t".join(cols)] + ["\t".join("" if v is None else (f"{v:.4g}" if isinstance(v, float) else str(v)) for v in r) for r in rows[:limit]]
    if len(rows) > limit:
        out.append(f"... more rows (raise --limit or narrow the query)")
    return out
