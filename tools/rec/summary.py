"""Per-category markdown summaries of one recording (<= ~60 lines each), cached in <rec>/summary/<cat>.md.

Categories: movement, combat, enemies, bosses, skills, items, camera, sound, effects, script, all (overview).
Every number here is measured from the real game; the summaries say which fields are not yet verified.
"""
import json
import math
import sqlite3
from collections import defaultdict
from pathlib import Path
from statistics import median

from .fields import FIELDS, GLOBAL_FIELDS, anim_name, map_name
from .ingest import ingest
from .names import goods, speffect

MAX_LINES = 60


def _db(rec):
    db = sqlite3.connect(ingest(rec))
    db.row_factory = sqlite3.Row
    return db


def _head(db, rec, cat):
    meta = {r["k"]: r["v"] for r in db.execute("SELECT k, v FROM meta")}
    secs = db.execute("SELECT (max(ms) - min(ms)) / 1000 FROM frames WHERE slot = 0").fetchone()[0] or 0
    note = meta.get("note", "")
    return [f"# {Path(rec).name} - {cat}", f"{secs:.0f} s recorded, {meta.get('hz', '60')} Hz. {note}".strip(), ""]


def _f(v, d=1):
    return "-" if v is None else f"{v:.{d}f}"


def _table(head, rows):
    out = ["| " + " | ".join(head) + " |", "|" + "---|" * len(head)]
    out += ["| " + " | ".join(str(c) for c in r) + " |" for r in rows]
    return out


def _yaw_rates(db, slot):
    """anim -> list of |yaw rate| deg/s per frame."""
    out = defaultdict(list)
    prev = None
    for r in db.execute("SELECT tick, ms, anim, yaw FROM frames WHERE slot = ? ORDER BY tick", (slot,)):
        if prev and r["yaw"] is not None and prev["yaw"] is not None and r["ms"] > prev["ms"]:
            d = (r["yaw"] - prev["yaw"] + math.pi) % (2 * math.pi) - math.pi
            out[r["anim"]].append(abs(math.degrees(d)) / ((r["ms"] - prev["ms"]) / 1000))
        prev = r
    return out


def movement(db, rec):
    L = _head(db, rec, "movement")
    speeds = defaultdict(list)
    for r in db.execute("SELECT anim, speed FROM frames WHERE slot = 0 AND speed IS NOT NULL"):
        speeds[r["anim"]].append(r["speed"])
    yaw = _yaw_rates(db, 0)
    rows = db.execute("""SELECT anim, count(*) n, avg(dur_ms) d, sum(dur_ms) tot, avg(abs(turn_deg)) tr, avg(lock) lk
                         FROM anims WHERE slot = 0 GROUP BY anim ORDER BY tot DESC LIMIT 22""").fetchall()
    L.append("Wolf anims by time. speed = median horizontal m/s over the anim's frames; turn = median / p90 yaw deg/s.")
    t = []
    for r in rows:
        sp = speeds.get(r["anim"], [])
        yr = sorted(yaw.get(r["anim"], []))
        t.append((anim_name(r["anim"]), r["n"], _f(r["d"], 0), _f(median(sp), 2) if sp else "-",
                  _f(median(yr), 0) if yr else "-", _f(yr[int(len(yr) * 0.9)], 0) if yr else "-", _f(r["lk"], 1)))
    L += _table(["anim", "n", "avg ms", "speed", "turn med", "turn p90", "locked"], t)
    falls = db.execute("SELECT tick, ms, text FROM events WHERE slot = 0 AND kind IN ('fall_start','fall_end') ORDER BY tick").fetchall()
    pairs = []
    for a, b in zip(falls, falls[1:]):
        if a["text"] is not None and a[0] < b[0] and a["ms"] < b["ms"]:
            ya = db.execute("SELECT y FROM frames WHERE slot=0 AND tick=?", (a["tick"],)).fetchone()
            yb = db.execute("SELECT y FROM frames WHERE slot=0 AND tick=?", (b["tick"],)).fetchone()
            pairs.append((a["tick"], a["text"], b["text"], _f(b["ms"] - a["ms"], 0), _f((ya[0] - yb[0]) if ya and yb else None, 2)))
    pairs = pairs[::2][:8]
    if pairs:
        L += ["", "Falls (vertical speed below -3 m/s): start tick, anim at start / end, ms, drop m"]
        L += _table(["tick", "start anim", "end anim", "ms", "drop"], pairs)
    gs = db.execute("SELECT DISTINCT round(game_speed, 3) FROM globals WHERE game_speed IS NOT NULL").fetchall()
    if gs and any(abs(g[0] - 1) > 1e-3 for g in gs):
        L += ["", f"Game speed values seen: {[g[0] for g in gs]} (timings above are wall-clock; slow-mo stretches them)"]
    return L


def combat(db, rec):
    L = _head(db, rec, "combat")
    for title, where in (("Hits on Wolf (by enemy attack anim)", "victim_slot = 0"),
                         ("Hits on enemies (by Wolf attack anim)", "victim_slot > 0")):
        rows = db.execute(f"""SELECT attacker_anim a, count(*) n, avg(hp_delta) hp, avg(posture_delta) po,
            avg(attacker_anim_ms) at, group_concat(DISTINCT victim_anim) va, group_concat(DISTINCT reaction) re
            FROM hits WHERE {where} AND (dist IS NULL OR dist < 15) GROUP BY a ORDER BY n DESC LIMIT 14""").fetchall()
        L += ["", f"{title}: hp / posture change per hit, ms into the attack anim, victim anim at hit -> 6 ticks later"]
        L += _table(["attack anim", "n", "hp", "posture", "at ms", "victim anim", "reaction"],
                    [(r["a"], r["n"], _f(r["hp"], 0), _f(r["po"], 0), _f(r["at"], 0), (r["va"] or "")[:40], (r["re"] or "")[:40]) for r in rows])
    # posture regen: rising runs per slot
    L += ["", "Posture recovery (rising runs >= 0.5 s): slot, points per second median, during anims"]
    for slot in (0, 1, 2):
        rates, anims = [], defaultdict(int)
        run = None
        for r in db.execute("SELECT ms, posture, anim FROM frames WHERE slot = ? AND posture IS NOT NULL ORDER BY tick", (slot,)):
            if run and r["posture"] > run["last"]:
                run["last"], run["ms1"] = r["posture"], r["ms"]
                anims[anim_name(r["anim"])] += 1
            else:
                if run and run["ms1"] - run["ms0"] >= 500:
                    rates.append((run["last"] - run["p0"]) / ((run["ms1"] - run["ms0"]) / 1000))
                run = {"p0": r["posture"], "last": r["posture"], "ms0": r["ms"], "ms1": r["ms"]}
        if rates:
            top = ", ".join(a for a, _ in sorted(anims.items(), key=lambda x: -x[1])[:4])
            L.append(f"- slot {slot}: {len(rates)} runs, {median(rates):.1f}/s (min {min(rates):.1f}, max {max(rates):.1f}); {top}")
    sp = db.execute("""SELECT s.id, count(*) n, avg(s.dur_ms) d FROM speffects s JOIN hits h
        ON s.slot = h.victim_slot AND s.start_tick BETWEEN h.tick - 2 AND h.tick + 4 GROUP BY s.id ORDER BY n DESC LIMIT 10""").fetchall()
    if sp:
        L += ["", "SpEffects added on the victim at a hit: " + ", ".join(f"{speffect(r['id'])} x{r['n']} ({_f(r['d'], 0)} ms)" for r in sp)]
    th = db.execute("SELECT id, other, who, anim, count(*) n FROM fx WHERE kind = 'throw' GROUP BY id, other, who ORDER BY n DESC LIMIT 10").fetchall()
    if th:
        L += ["", "Throws / grabs / deathblows (throw hook, ThrowParam id: thrower -> victim, thrower anim): "
              + "; ".join(f"{r['id']}: {r['other'] or '?'} -> {r['who'] or '?'} ({r['anim'] or '-'}) x{r['n']}" for r in th)]
    hk = db.execute("""SELECT other att, who vic, anim, count(*) n, avg(at_ms) at FROM fx WHERE kind = 'hit'
        GROUP BY att, vic, anim ORDER BY n DESC LIMIT 12""").fetchall()
    if hk:
        L += ["", "Hitbox contacts (hit hook FUN_140995470, one per contact; includes guarded / deflected / whiffed-through checks):"]
        L += _table(["attacker", "victim", "attacker anim", "n", "at ms"],
                    [(r["att"] or "?", r["vic"] or "?", r["anim"] or "-", r["n"], _f(r["at"], 0)) for r in hk])
    return L


def enemies(db, rec, title="enemies"):
    L = _head(db, rec, title)
    # most active first (anim changes + hits taken); characters that never changed anim -> one "idle" line
    allw = db.execute("""SELECT f.who, count(*) n, max(hp_max) hm, max(posture_max) pm, min(hp) hpmin,
        (SELECT count(*) FROM anims a WHERE a.who = f.who) na, (SELECT count(*) FROM hits h WHERE h.victim = f.who) nh
        FROM frames f WHERE slot > 0 GROUP BY f.who ORDER BY na + nh DESC""").fetchall()
    whos = [w for w in allw if w["na"] > 2 or w["nh"] > 0][:4]
    idle = [w["who"] for w in allw if not (w["na"] > 2 or w["nh"] > 0)]
    if idle:
        L += [f"Idle nearby (never acted): {', '.join(idle[:8])}", ""]
    budget = (MAX_LINES - 6) // max(1, len(whos))
    for w in whos:
        part = [f"## {w['who']}  ({w['n'] / 60:.0f} s tracked, hp max {w['hm']}, posture max {w['pm']}, lowest hp {w['hpmin']})"]
        rows = db.execute("""SELECT name, count(*) n, avg(dur_ms) d, avg(speed) sp FROM anims WHERE who = ?
            GROUP BY name ORDER BY n DESC LIMIT 10""", (w["who"],)).fetchall()
        part.append("anims (n, avg ms, avg m/s): " + "; ".join(f"{r['name']} {r['n']}x {_f(r['d'], 0)} {_f(r['sp'], 1)}" for r in rows))
        acts = db.execute("""SELECT e.tick, e.a, e.b,
              (SELECT sqrt((f.x - p.x) * (f.x - p.x) + (f.z - p.z) * (f.z - p.z)) FROM frames f, frames p
               WHERE f.slot = e.slot AND f.tick = e.tick AND p.slot = 0 AND p.tick = e.tick) dist
            FROM events e WHERE e.who = ? AND e.kind = 'ai_act' ORDER BY e.tick""", (w["who"],)).fetchall()
        if acts:
            cnt = defaultdict(list)
            for a in acts:
                cnt[a["a"]].append(a["dist"])
            part.append("AI acts (act: times, median dist to Wolf m): " + "; ".join(
                f"{k}: {len(v)}x {_f(median([d for d in v if d is not None]) if any(d is not None for d in v) else None, 1)}"
                for k, v in sorted(cnt.items(), key=lambda x: -len(x[1]))[:12]))
            part.append("act sequence: " + " ".join(str(a["a"]) for a in acts[:40]))
        cov = _move_coverage(db, w["who"])
        if cov:
            part.append(cov)
        hits = db.execute("""SELECT count(*), avg(hp_delta), avg(posture_delta) FROM hits WHERE victim = ?""", (w["who"],)).fetchone()
        part.append(f"hit by Wolf: {hits[0]}x, avg hp {_f(hits[1], 0)}, avg posture {_f(hits[2], 0)}")
        sp = db.execute("SELECT id, count(*) n FROM speffects WHERE who = ? GROUP BY id ORDER BY n DESC LIMIT 12", (w["who"],)).fetchall()
        if sp:
            part.append("SpEffects: " + ", ".join(f"{speffect(r['id'])} x{r['n']}" for r in sp))
        L += part[:budget] + [""]
    if any(r["name"] == "ai_goal" for r in db.execute("SELECT DISTINCT name FROM hooks")):
        g = db.execute("SELECT a1, count(*) n FROM hooks WHERE name = 'ai_goal' GROUP BY a1 ORDER BY n DESC LIMIT 15").fetchall()
        L.append("AddSubGoal ids (all enemies): " + ", ".join(f"{r['a1']} x{r['n']}" for r in g))
    return L


CHR_DIR = Path(__file__).resolve().parents[3] / "shinobi-combat" / "extracted" / "chr"


def _siblings(rec):
    """Other parts of the same test: rec_<date>_<time>_<name>[_b|_c...] with the same <name>."""
    import re
    rec = Path(rec)
    m = re.match(r"rec_\d{8}_\d{6}_(.+?)(?:_[a-z])?$", rec.name)
    if not m:
        return []
    out = []
    for p in rec.parent.glob("rec_*"):
        q = re.match(r"rec_\d{8}_\d{6}_(.+?)(?:_[a-z])?$", p.name)
        if p != rec and q and q.group(1) == m.group(1) and (p / "frames.bin").exists():
            out.append(p)
    return sorted(out)


CURRENT_REC = [None]


def _move_coverage(db, who):
    """'attacks seen X of Y' against the character's anibnd (a000_003xxx = attack anims), over all parts."""
    import re
    m = re.search(r":c(\d{4})\d*:", who)
    if not m:
        return None
    d = CHR_DIR / f"c{m.group(1)}.anibnd.d"
    if not d.exists():
        return None
    atk = {p.stem for p in d.glob("a000_003*.hkx")}
    if not atk:
        return None
    q = "SELECT DISTINCT name FROM anims WHERE who LIKE ?"
    seen = {r[0] for r in db.execute(q, (f"%:c{m.group(1)}%",))} & atk
    here = len(seen)
    parts = _siblings(CURRENT_REC[0]) if CURRENT_REC[0] else []
    for p in parts:
        o = sqlite3.connect(ingest(p))
        seen |= {r[0] for r in o.execute(q, (f"%:c{m.group(1)}%",))} & atk
        o.close()
    miss = sorted(a[5:] for a in atk - seen)
    both = f" ({here} in this part, {len(parts) + 1} parts together)" if parts else ""
    return (f"attacks seen {len(seen)} of {len(atk)}{both} (c{m.group(1)} anibnd a000_003xxx); missing: "
            + " ".join(miss[:45]) + (" ..." if len(miss) > 45 else ""))


def bosses(db, rec):
    return enemies(db, rec, "bosses")


def _flag_warnings(db):
    g = db.execute("SELECT max(dbg_nodead), max(dbg_no_goods), max(dbg_no_emblems) FROM globals").fetchone()
    w = [n for n, v in zip(("can't-die", "infinite items", "infinite Spirit Emblems"), g or ()) if v]
    return [f"WARNING: switched on during this recording: {', '.join(w)} (item / emblem counts and deaths are not real)", ""] if w else []


# Wolf anim groups (c0000 TAE section = anim id // 1000000)
GROUPS = [("prosthetic tools (a070-a079)", 70, 79), ("combat arts (a100-a199)", 100, 199),
          ("deathblows / throws (a200-a299)", 200, 299), ("guard / deflect (a050)", 50, 50)]


def skills(db, rec):
    L = _head(db, rec, "skills / arts / prosthetics") + _flag_warnings(db)
    segs = db.execute("SELECT anim, name, start_tick, end_tick, dur_ms FROM anims WHERE slot = 0").fetchall()
    sps = db.execute("SELECT id, start_tick FROM speffects WHERE slot = 0").fetchall()
    total = defaultdict(int)
    for sp in sps:
        total[sp["id"]] += 1
    import bisect
    starts = sorted((r["start_tick"], r["end_tick"] if r["end_tick"] is not None else r["start_tick"], r["anim"]) for r in segs)
    keys = [x[0] for x in starts]

    def anim_at(t):
        i = bisect.bisect_right(keys, t) - 1
        return starts[i][2] if i >= 0 and starts[i][1] >= t else None

    sp_anim = defaultdict(lambda: defaultdict(int))  # anim -> sp id -> n
    for sp in sps:
        a = anim_at(sp["start_tick"])
        if a is not None:
            sp_anim[a][sp["id"]] += 1
    for title, lo, hi in GROUPS:
        rows = [r for r in db.execute("""SELECT anim, name, count(*) n, avg(dur_ms) d FROM anims WHERE slot = 0 AND
            anim / 1000000 BETWEEN ? AND ? GROUP BY anim ORDER BY anim""", (lo, hi))]
        if not rows:
            continue
        L.append(f"## {title}: anim, times, avg ms; SpEffects mostly seen in that anim (n of all)")
        by_sec = defaultdict(list)
        for r in rows:
            by_sec[r["anim"] // 1000].append(r)
        for sec, rs in sorted(by_sec.items()):
            spec = defaultdict(int)
            for r in rs:
                for i, n in sp_anim[r["anim"]].items():
                    spec[i] += n
            own = sorted(((n, i) for i, n in spec.items() if n >= 0.6 * total[i]), reverse=True)[:4]
            hits = db.execute(f"""SELECT count(*), avg(hp_delta), avg(posture_delta) FROM hits WHERE attacker_slot = 0
                AND attacker_anim IN ({",".join("?" * len(rs))})""", [r["name"] for r in rs]).fetchone()
            line = "- " + " ".join(f"{r['name']} {r['n']}x {_f(r['d'], 0)}" for r in rs[:5])
            if hits[0]:
                line += f" | hits {hits[0]}: hp {_f(hits[1], 0)} posture {_f(hits[2], 0)}"
            if own:
                line += " | " + ", ".join(f"{speffect(i)} ({n}/{total[i]})" for n, i in own)
            L.append(line[:330])
        L.append("")
    items = db.execute("SELECT tick, a1, a2, item FROM hooks WHERE name = 'items' ORDER BY tick LIMIT 20").fetchall()
    if items:
        L += ["Item count changes (tick item delta): " + "; ".join(
            f"{r['tick']} {goods(r['item']) if r['item'] is not None else 'slot ' + str(r['a1'])} {r['a2']}" for r in items)]
    return L


def items(db, rec):
    L = _head(db, rec, "items") + _flag_warnings(db)
    rows = db.execute("SELECT tick, a1 slot, a2 delta, item, count FROM hooks WHERE name = 'items' ORDER BY tick").fetchall()
    if not rows:
        L.append("No item hook data (record with hooks=items). gap: inventory layout not decoded yet.")
        return L
    L.append("Per use: item (AdjustItemCount slot -> goods id; old DLL: slot only), count before, delta, Wolf anim + how long it ran, HP change in the next 3 s, SpEffects added within 1.5 s")
    t = []
    for r in rows[:35]:
        f = db.execute("SELECT ms, hp, anim FROM frames WHERE slot = 0 AND tick <= ? ORDER BY tick DESC LIMIT 1", (r["tick"],)).fetchone()
        if not f:
            continue
        g = db.execute("SELECT max(hp) FROM frames WHERE slot = 0 AND ms BETWEEN ? AND ?", (f["ms"], f["ms"] + 3000)).fetchone()
        an = db.execute("SELECT name, dur_ms FROM anims WHERE slot = 0 AND start_tick <= ? ORDER BY start_tick DESC LIMIT 1", (r["tick"],)).fetchone()
        sp = db.execute("SELECT id, dur_ms FROM speffects WHERE slot = 0 AND start_tick BETWEEN ? AND ?", (r["tick"] - 30, r["tick"] + 90)).fetchall()
        t.append((r["tick"], goods(r["item"]) if r["item"] is not None else f"slot {r['slot']}", r["count"] if r["count"] is not None else "-", r["delta"], f"{an['name']} {_f(an['dur_ms'], 0)}" if an else "-",
                  (g[0] - f["hp"]) if g[0] is not None and f["hp"] is not None else "-",
                  ", ".join(f"{speffect(s['id'])} ({_f(s['dur_ms'], 0)})" for s in sp)[:90]))
    L += _table(["tick", "item", "count", "delta", "anim ms", "hp +", "speffects"], t)
    return L


def camera(db, rec):
    L = _head(db, rec, "camera")
    ons = db.execute("SELECT count(*) FROM events WHERE slot = 0 AND kind = 'lock_flag' AND a = 1").fetchone()[0]
    tot = db.execute("SELECT count(*), sum(lock_flag = 1) FROM frames WHERE slot = 0").fetchone()
    L.append(f"Lock-on: turned on {ons} times; locked {100 * (tot[1] or 0) / max(1, tot[0]):.0f}% of frames (ChrIns +0x1070).")
    lk = db.execute("SELECT tick, ms, who FROM fx WHERE kind = 'lock' ORDER BY tick LIMIT 30").fetchall()
    if lk:
        L.append("Locked target changes (lock hook, ms who): " + "; ".join(f"{r['ms'] / 1000:.1f}s {r['who'] or 'none'}" for r in lk))
    else:
        L.append("gap: which enemy is targeted (record with the lock hook).")
    ver = GLOBAL_FIELDS["cam_follow_yaw"][3]
    rows = db.execute("SELECT ms, cam_follow_yaw y, cam_follow_pitch p FROM globals WHERE cam_follow_yaw IS NOT NULL ORDER BY tick").fetchall()
    if rows:
        rates = []
        for a, b in zip(rows, rows[1:]):
            if b["ms"] > a["ms"]:
                d = (b["y"] - a["y"] + math.pi) % (2 * math.pi) - math.pi
                rates.append(abs(math.degrees(d)) / ((b["ms"] - a["ms"]) / 1000))
        rates.sort()
        if rates:
            L.append(f"Follow-cam yaw rate deg/s: median {rates[len(rates) // 2]:.0f}, p90 {rates[int(len(rates) * .9)]:.0f}, max {rates[-1]:.0f}"
                     + ("" if ver else "  (field not verified yet)"))
        pit = [r["p"] for r in rows if r["p"] is not None]
        if pit:
            L.append(f"Follow-cam pitch range: {min(pit):.3f} .. {max(pit):.3f}" + ("" if ver else "  (field not verified yet)"))
    cam = db.execute("""SELECT g.cam_x cx, g.cam_y cy, g.cam_z cz, g.cam_fov fov, f.x, f.y, f.z, f.lock_flag lk
        FROM globals g JOIN frames f ON f.slot = 0 AND f.tick = g.tick WHERE g.cam_x IS NOT NULL""").fetchall()
    if cam:
        for title, sel in (("free", [r for r in cam if not r["lk"]]), ("locked on", [r for r in cam if r["lk"]])):
            if not sel:
                continue
            dist = sorted(math.hypot(r["cx"] - r["x"], r["cz"] - r["z"]) for r in sel)
            up = sorted(r["cy"] - r["y"] for r in sel)
            L.append(f"Camera {title} ({len(sel)} frames): behind Wolf {dist[len(dist) // 2]:.2f} m median "
                     f"({dist[len(dist) // 10]:.2f}..{dist[int(len(dist) * .9)]:.2f}), above his feet {up[len(up) // 2]:.2f} m "
                     f"({up[len(up) // 10]:.2f}..{up[int(len(up) * .9)]:.2f})")
        fovs = sorted({round(r["fov"], 4) for r in cam if r["fov"]})
        L.append(f"Field of view (radians): {', '.join(map(str, fovs[:6]))}")
    return L


def sound(db, rec):
    L = _head(db, rec, "sound / HUD")
    rows = db.execute("SELECT tick, a1 FROM hooks WHERE name = 'music' ORDER BY tick LIMIT 40").fetchall()
    if rows:
        L.append("StartMusic ids (tick id): " + "; ".join(f"{r['tick']} {r['a1']}" for r in rows))
    else:
        L.append("No music hook data (record with hooks=music).")
    rows = db.execute("""SELECT who, anim, sub, id, count(*) n, avg(at_ms) at FROM fx WHERE kind = 'sound'
        GROUP BY who, anim, sub, id ORDER BY who, anim, at LIMIT 45""").fetchall()
    if rows:
        L += ["", "Sounds played on characters (sound hook FUN_1409cf930): who, anim, type:id, times, ms into the anim"]
        L += _table(["who", "anim", "sound", "n", "at ms"],
                    [(r["who"] or "?", r["anim"] or "-", f"{r['sub']}:{r['id']}", r["n"], _f(r["at"], 0)) for r in rows])
    else:
        L.append("No sound hook data (record with the sound hook).")
    L.append("gap: HUD state not recorded; sounds not attached to a character are not hooked.")
    return L


def _hp_pct(db, t):
    """'boss hp 63%' of the slot-1 enemy at tick t (phase changes show as scripted commands near hp 0)."""
    r = db.execute("SELECT hp, hp_max, who FROM frames WHERE slot = 1 AND tick <= ? AND hp_max > 0 ORDER BY tick DESC LIMIT 1", (t,)).fetchone()
    return f"{r['who'].split(':')[1]} {100 * r['hp'] / r['hp_max']:.0f}%" if r else "-"


def script(db, rec):
    L = _head(db, rec, "script (event scripts, event flags, boss phases)")
    rows = db.execute("""SELECT tick, ms, id ev, sub, other FROM fx WHERE kind = 'emevd' ORDER BY tick""").fetchall()
    flags = db.execute("SELECT tick, ms, id, sub FROM fx WHERE kind = 'flag' ORDER BY tick").fetchall()
    if not rows and not flags:
        L.append("No script hook data (record with the emevd / flag hooks).")
        return L
    # commands repeated many times (per-frame loops) are counted, not listed
    count = defaultdict(int)
    for r in rows:
        count[(r["ev"], r["sub"])] += 1
    L += ["Timeline (s, slot-1 enemy hp, what). EMEVD action commands (event id: command) and event flag changes;",
          "commands that ran more than 3 times are in the table below instead."]
    items = [(r["tick"], r["ms"], f"event {r['ev']}: {r['other']}") for r in rows if count[(r["ev"], r["sub"])] <= 3]
    items += [(r["tick"], r["ms"], f"flag {r['id']} = {r['sub']}") for r in flags]
    items.sort()
    for t, ms, what in items[:34]:
        L.append(f"- {ms / 1000:6.1f}s  {_hp_pct(db, t):>14}  {what[:110]}")
    if len(items) > 34:
        L.append(f"- ... {len(items) - 34} more: `query <rec> sql \"SELECT ms, kind, id, other FROM fx WHERE kind IN ('emevd','flag')\"`")
    rep = sorted(((n, k) for k, n in count.items() if n > 3), reverse=True)[:10]
    if rep:
        L += ["", "Repeated commands (times, event id, command):"]
        names = {(r["ev"], r["sub"]): r["other"] for r in rows}
        L += [f"- x{n} event {k[0]}: {names[k][:100]}" for n, k in rep]
    return L


def effects(db, rec):
    L = _head(db, rec, "effects (VFX, bullets, TAE events, HKS events)")
    rows = db.execute("""SELECT who, anim, id, count(*) n, avg(at_ms) at FROM fx WHERE kind = 'vfx'
        GROUP BY who, anim, id ORDER BY n DESC LIMIT 18""").fetchall()
    if rows:
        L += ["VFX created (vfx hook FUN_14092afe0; who = nearest character within 4 m): who, anim, FFX id, times, ms into the anim"]
        L += _table(["who", "anim", "ffx", "n", "at ms"], [(r["who"] or "?", r["anim"] or "-", r["id"], r["n"], _f(r["at"], 0)) for r in rows])
    b = db.execute("SELECT id, count(*) n FROM fx WHERE kind = 'bullet' GROUP BY id ORDER BY n DESC LIMIT 15").fetchall()
    if b:
        L += ["", "Bullets created (bullet hook, BulletParam id x times): " + ", ".join(f"{r['id']} x{r['n']}" for r in b)]
    t = db.execute("""SELECT who, type, count(*) n, count(DISTINCT anim) na, avg(dur_ms) d FROM tae
        GROUP BY who, type ORDER BY who = 'wolf' DESC, who, n DESC""").fetchall()
    if t:
        per = defaultdict(list)
        for r in t:
            per[r["who"]].append(f"{r['type']} x{r['n']}")
        L += ["", "TAE events run (type x times) per character - details: `query <rec> sql \"SELECT * FROM tae WHERE type = N\"`"]
        for w, v in sorted(per.items(), key=lambda x: (x[0] != "wolf", -len(x[1])))[:8]:
            L.append(f"- {w}: " + ", ".join(v[:24]))
    h = db.execute("SELECT other, count(*) n FROM fx WHERE kind = 'hks' GROUP BY other ORDER BY n DESC LIMIT 30").fetchall()
    if h:
        L += ["", "HKS events fired (hkbFireEvent, all characters): " + ", ".join(f"{r['other']} x{r['n']}" for r in h)]
    if len(L) <= 3:
        L.append("No effect hook data (record with the hit / tae / bullet / vfx / hks hooks).")
    return L


def overview(db, rec):
    L = _head(db, rec, "overview") + _flag_warnings(db)
    for t in ("frames", "events", "anims", "hits", "speffects", "hooks", "tae", "fx"):
        L.append(f"- {t}: {db.execute(f'SELECT count(*) FROM {t}').fetchone()[0]} rows")
    marks = db.execute("SELECT tick, ms, text FROM events WHERE kind IN ('mark', 'cmd') ORDER BY tick LIMIT 25").fetchall()
    if marks:
        L += ["", "Markers / commands: " + "; ".join(f"{r['tick']} {r['text']}" for r in marks)]
    maps = db.execute("SELECT map_block, count(*) FROM globals WHERE map_block IS NOT NULL GROUP BY map_block").fetchall()
    if maps:
        L += ["", "Maps: " + ", ".join(f"{map_name(m[0])} ({m[1] / 60:.0f} s)" for m in maps)]
    whos = db.execute("SELECT who, count(*) FROM frames GROUP BY who").fetchall()
    L += ["", "Characters: " + ", ".join(f"{w[0]} ({w[1] / 60:.0f} s)" for w in whos)]
    bad = [k for k, v in {**FIELDS, **GLOBAL_FIELDS}.items() if not v[3]]
    L += ["", "Fields not verified live yet: " + ", ".join(bad)]
    return L


CATS = {"movement": movement, "combat": combat, "enemies": enemies, "bosses": bosses, "skills": skills,
        "items": items, "camera": camera, "sound": sound, "effects": effects, "script": script, "all": overview}


def summary(rec, cat, write=True):
    CURRENT_REC[0] = rec
    db = _db(rec)
    lines = CATS[cat](db, rec)
    if len(lines) > MAX_LINES:
        lines = lines[:MAX_LINES - 1] + [f"... cut at {MAX_LINES} lines; drill down with `query`"]
    text = "\n".join(lines) + "\n"
    if write:
        d = Path(rec) / "summary"
        d.mkdir(exist_ok=True)
        (d / f"{cat}.md").write_text(text, encoding="utf-8")
    return text
