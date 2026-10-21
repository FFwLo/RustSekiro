"""frames.bin + events.jsonl + hooks.jsonl -> rec.db (SQLite) with decoded frames and derived tables."""
import json
import math
import sqlite3
from collections import defaultdict
from pathlib import Path

from . import raw
from .fields import FACE_LAYER, FIELDS, GLOBAL_FIELDS, anim_name
from .names import emevd

SCHEMA = """
CREATE TABLE meta(k TEXT PRIMARY KEY, v TEXT);
CREATE TABLE frames(tick INT, ms REAL, slot INT, who TEXT, anim INT, x REAL, y REAL, z REAL, yaw REAL,
  speed REAL, vy REAL, hp INT, hp_max INT, posture INT, posture_max INT, ai_act INT, ai_kengeki INT,
  ai_force_act INT, btn_attack INT, btn_guard INT, lock_flag INT, lock_target INT, poison REAL, burn REAL,
  shock REAL, btn_jump INT, btn_step INT, btn_crouch INT, btn_item INT, btn_prosthetic INT, btn_art INT,
  sprint INT, move_x REAL, move_z REAL, stick_x REAL, stick_y REAL, rm_scale REAL, PRIMARY KEY(slot, tick));
CREATE TABLE globals(tick INT PRIMARY KEY, ms REAL, game_speed REAL, igt INT, cam_follow_yaw REAL,
  cam_follow_pitch REAL, dbg_nodead INT, dbg_freezeai INT, dbg_no_goods INT, dbg_no_emblems INT, map_block INT,
  cam_x REAL, cam_y REAL, cam_z REAL, cam_fwd_x REAL, cam_fwd_y REAL, cam_fwd_z REAL, cam_fov REAL,
  cam_yaw REAL, cam_pitch REAL);
CREATE TABLE events(tick INT, ms REAL, slot INT, who TEXT, kind TEXT, a INT, b INT, text TEXT);
CREATE TABLE anims(slot INT, who TEXT, anim INT, name TEXT, start_tick INT, end_tick INT, start_ms REAL,
  dur_ms REAL, dist REAL, speed REAL, turn_deg REAL, dy REAL, prev TEXT, next TEXT, lock INT);
CREATE TABLE hits(tick INT, ms REAL, victim_slot INT, victim TEXT, hp_delta INT, posture_delta INT,
  hp_after INT, posture_after INT, victim_anim TEXT, reaction TEXT, attacker_slot INT, attacker_anim TEXT,
  attacker_anim_ms REAL, dist REAL);
CREATE TABLE speffects(slot INT, who TEXT, id INT, start_tick INT, end_tick INT, dur_ms REAL, anim TEXT);
CREATE TABLE hooks(tick INT, name TEXT, a1 INT, a2 INT, a3 INT, item INT, count INT, json TEXT);
CREATE TABLE tae(tick INT, end_tick INT, ms REAL, dur_ms REAL, who TEXT, type INT, anim TEXT, at_ms REAL, args TEXT);
CREATE TABLE fx(tick INT, ms REAL, kind TEXT, who TEXT, id INT, sub INT, anim TEXT, at_ms REAL, other TEXT, raw TEXT);
CREATE INDEX tae_type ON tae(type, who);
CREATE INDEX fx_kind ON fx(kind, id);
CREATE INDEX ev_kind ON events(kind, tick);
CREATE INDEX an_name ON anims(name);
CREATE TABLE clips(tick INT, ms REAL, name TEXT, anim INT, t REAL, w REAL, speed REAL, at_end INT, rm TEXT);
CREATE INDEX cl_tick ON clips(tick);
CREATE VIEW hks_events AS SELECT tick, ms, other AS event, raw AS lua FROM fx WHERE kind = 'hks';
"""

BUTTON_BITS = {"btn_prosthetic": 0x02, "btn_jump": 0x10, "btn_step": 0x20, "btn_item": 0x80, "btn_crouch": 0x8000}
PER_SLOT = [k for k in FIELDS if k != "handle"]
SCHEMA_VERSION = "14"  # bump when tables change: older rec.db files are rebuilt automatically


def _current(db_path, rec):
    if not db_path.exists() or db_path.stat().st_mtime < (rec / "frames.bin").stat().st_mtime:
        return False
    db = sqlite3.connect(db_path)
    try:
        v = db.execute("SELECT v FROM meta WHERE k = 'schema'").fetchone()
        return v is not None and v[0] == SCHEMA_VERSION
    except sqlite3.Error:
        return False
    finally:
        db.close()


def _jsonl(path):
    if not path.exists():
        return []
    out = []
    for line in path.read_text(encoding="utf-8", errors="replace").splitlines():
        line = line.strip()
        if line:
            try:
                out.append(json.loads(line))
            except json.JSONDecodeError:
                pass
    return out


def ingest(rec, force=False):
    rec = Path(rec)
    db_path = rec / "rec.db"
    if not force and _current(db_path, rec):
        return db_path
    if db_path.exists():
        db_path.unlink()
    names = raw.block_names(rec)
    ids = {v: k for k, v in names.items()}
    events = _jsonl(rec / "events.jsonl")

    # who occupies slot 1/2 over time (rec2 "slot" events); old recordings: just the slot number
    slot_changes = defaultdict(list)  # slot 1.. (rec2 args.enemies, 6 since 2026-10-21)
    for e in events:
        if e.get("kind") == "slot":
            slot_changes[e["slot"]].append((e["tick"], f"{e['slot']}:c{e.get('chr_id', -1)}:{e.get('handle', -1)}"))

    def who(slot, tick):
        if slot == 0:
            return "wolf"
        cur = f"{slot}:?"
        for t, w in slot_changes.get(slot, []):
            if t <= tick:
                cur = w
            else:
                break
        return cur

    by_block = {}
    for name, (blk, off, ty, *_rest) in FIELDS.items():
        if blk in ids:
            by_block.setdefault(ids[blk], []).append((name, off, ty))
    gby_block = {}
    for name, (blk, off, ty, *_rest) in GLOBAL_FIELDS.items():
        if blk in ids:
            gby_block.setdefault(ids[blk], []).append((name, off, ty))

    frames = {}   # (slot, tick) -> dict
    globs = {}
    last = {}     # slot -> carried dict (slow blocks)
    body = {}     # slot -> last body anim
    for tick, ms, slot, block, b in raw.records(rec):
        if slot == 255:
            g = globs.setdefault(tick, {"ms": ms})
            for name, off, ty in gby_block.get(block, ()):
                g[name] = raw.value(b, off, ty)
            continue
        row = frames.get((slot, tick))
        if row is None:
            row = dict(last.get(slot, {}))
            row["ms"], row["tick"] = ms, tick
            frames[(slot, tick)] = row
        for name, off, ty in by_block.get(block, ()):
            v = raw.value(b, off, ty)
            if name == "anim":
                if v is not None and not (v < 1000000 and v in FACE_LAYER):
                    body[slot] = v
                v = body.get(slot, v)
            row[name] = v
        last[slot] = row

    db = sqlite3.connect(db_path)
    db.executescript(SCHEMA)
    meta = {}
    for mf in ("manifest.json", "manifest_lua.json"):
        p = rec / mf
        if p.exists():
            try:
                for k, v in json.loads(p.read_text()).items():
                    if k != "blocks":
                        meta[k] = json.dumps(v) if not isinstance(v, str) else v
            except json.JSONDecodeError:
                pass
    db.executemany("INSERT INTO meta VALUES(?,?)", meta.items())

    # frames + per-frame horizontal speed / vertical speed (central difference over +-3 ticks)
    out_rows = []
    ev_btn, prev_btn = [], {}
    slots = sorted({s for s, _ in frames})
    per_slot = {s: sorted(t for ss, t in frames if ss == s) for s in slots}
    for s in slots:
        ticks = per_slot[s]
        for i, t in enumerate(ticks):
            r = frames[(s, t)]
            a, c = ticks[max(0, i - 3)], ticks[min(len(ticks) - 1, i + 3)]
            ra, rc = frames[(s, a)], frames[(s, c)]
            dt = (rc["ms"] - ra["ms"]) / 1000
            sp = vy = None
            if dt > 0 and None not in (ra.get("x"), rc.get("x"), ra.get("z"), rc.get("z")):
                sp = math.hypot(rc["x"] - ra["x"], rc["z"] - ra["z"]) / dt
                if sp > 60:  # teleport / load
                    sp = None
                vy = (rc["y"] - ra["y"]) / dt if None not in (ra.get("y"), rc.get("y")) else None
            r["speed"], r["vy"] = sp, vy
            h, h2, ms_ = r.get("ar_held"), r.get("ar_held2"), r.get("move_state")
            if h is not None:
                for k, bit in BUTTON_BITS.items():
                    r[k] = int(bool(h & bit))
            if h2 is not None:
                r["btn_art"] = h2 & 1
            if ms_ is not None:
                r["sprint"] = int(bool(ms_ & 4))
            # stick = move input in camera space (y forward, x right), from the camera's forward row
            g = globs.get(t) or {}
            mx, mz, fx, fz = r.get("move_x"), r.get("move_z"), g.get("cam_fwd_x"), g.get("cam_fwd_z")
            if None not in (mx, mz, fx, fz) and math.hypot(fx, fz) > 1e-6:
                n = math.hypot(fx, fz)
                fx, fz = fx / n, fz / n
                r["stick_y"] = mx * fx + mz * fz
                r["stick_x"] = mx * fz - mz * fx
            if s == 0:
                for k in (*BUTTON_BITS, "btn_art", "btn_attack", "btn_guard", "sprint"):
                    v, pv = r.get(k), prev_btn.get(k)
                    if v is not None and pv is not None and bool(v) != bool(pv):
                        ev_btn.append((t, r["ms"], 0, "wolf", "press" if v else "release", None, None, k[4:] if k.startswith("btn_") else k))
                    if v is not None:
                        prev_btn[k] = v
            out_rows.append((t, r["ms"], s, who(s, t), *[r.get(k) for k in ("anim", "x", "y", "z", "yaw", "speed", "vy",
                             "hp", "hp_max", "posture", "posture_max", "ai_act", "ai_kengeki", "ai_force_act",
                             "btn_attack", "btn_guard", "lock_flag", "lock_target", "poison", "burn", "shock",
                             "btn_jump", "btn_step", "btn_crouch", "btn_item", "btn_prosthetic", "btn_art", "sprint",
                             "move_x", "move_z", "stick_x", "stick_y", "rm_scale")]))
    db.executemany(f"INSERT INTO frames VALUES({','.join('?' * 37)})", out_rows)
    # camera yaw / pitch (degrees) from ChrCam's forward row: yaw 0 = +z, 90 = +x; pitch up > 0
    for g in globs.values():
        fx, fy, fz = g.get("cam_fwd_x"), g.get("cam_fwd_y"), g.get("cam_fwd_z")
        if None not in (fx, fy, fz):
            g["cam_yaw"] = math.degrees(math.atan2(fx, fz))
            g["cam_pitch"] = math.degrees(math.asin(max(-1.0, min(1.0, fy))))
    db.executemany(f"INSERT INTO globals VALUES({','.join('?' * 20)})",
                   [(t, g["ms"], g.get("game_speed"), g.get("igt"), g.get("cam_follow_yaw"), g.get("cam_follow_pitch"),
                     g.get("dbg_nodead"), g.get("dbg_freezeai"), g.get("dbg_no_goods"), g.get("dbg_no_emblems"), g.get("map_block"),
                     *[g.get(k) for k in ("cam_x", "cam_y", "cam_z", "cam_fwd_x", "cam_fwd_y", "cam_fwd_z", "cam_fov",
                                          "cam_yaw", "cam_pitch")]) for t, g in sorted(globs.items())])

    ev_rows = list(ev_btn)
    # recorder events
    sp_open = {}
    sp_rows = []
    for e in events:
        k, t, ms = e.get("kind"), e.get("tick", 0), e.get("ms", 0.0)
        s = e.get("slot")
        if k == "sp_add":
            sp_open[(s, e["id"])] = (t, ms)
        elif k == "sp_del" and (s, e["id"]) in sp_open:
            t0, ms0 = sp_open.pop((s, e["id"]))
            sp_rows.append((s, who(s, t0), e["id"], t0, t, ms - ms0, None))
        text = e.get("text") or e.get("line") or e.get("name") or e.get("err")
        ev_rows.append((t, ms, s, who(s, t) if s is not None else None, k, e.get("id", e.get("handle")), e.get("chr_id"), text))
    end_tick = max((t for _, t in frames), default=0)
    for (s, i), (t0, ms0) in sp_open.items():
        sp_rows.append((s, who(s, t0), i, t0, None, None, None))

    # derived events + anim segments + hits
    an_rows, hit_rows = [], []
    cur_anim = {}
    for s in slots:
        ticks = per_slot[s]
        seg = None
        prev_name = "-"
        prev = None
        for t in ticks:
            r = frames[(s, t)]
            w = who(s, t)
            if prev is not None:
                for k, kind in (("hp", "hp"), ("posture", "posture"), ("lock_target", "lock"), ("ai_act", "ai_act"),
                                ("ai_kengeki", "ai_kengeki"), ("lock_flag", "lock_flag")):
                    if r.get(k) != prev.get(k) and prev.get(k) is not None and r.get(k) is not None:
                        ev_rows.append((t, r["ms"], s, w, kind, r[k], prev[k], None))
                vy, pvy = r.get("vy"), prev.get("vy")
                if vy is not None and pvy is not None:
                    if pvy > -3 >= vy:
                        ev_rows.append((t, r["ms"], s, w, "fall_start", None, None, anim_name(r.get("anim"))))
                    if pvy <= -3 < vy:
                        ev_rows.append((t, r["ms"], s, w, "fall_end", None, None, anim_name(r.get("anim"))))
            a = r.get("anim")
            if seg is None or a != seg["anim"] or w != seg["who"]:
                if seg is not None:
                    an_rows.append(_segment(seg, prev, prev_name, anim_name(a) if w == seg["who"] else "-"))
                    prev_name = anim_name(seg["anim"]) if w == seg["who"] else "-"
                    ev_rows.append((t, r["ms"], s, w, "anim", a, seg["anim"], anim_name(a)))
                seg = {"anim": a, "who": w, "slot": s, "t0": t, "first": r}
            cur_anim.setdefault(s, {})[t] = (a, seg["first"]["ms"])
            prev = r
        if seg is not None:
            an_rows.append(_segment(seg, prev, prev_name, "-"))

    # hits: HP or posture drop on a slot; attacker = Wolf for enemies, nearest enemy slot for Wolf
    for s in slots:
        ticks = per_slot[s]
        for i in range(1, len(ticks)):
            t, pt = ticks[i], ticks[i - 1]
            r, p = frames[(s, t)], frames[(s, pt)]
            dhp = (r.get("hp") or 0) - (p.get("hp") or 0)
            dpo = (r.get("posture") or 0) - (p.get("posture") or 0)
            if dhp < 0 or dpo < 0:
                if who(s, t) != who(s, pt):
                    continue
                later = frames.get((s, ticks[min(len(ticks) - 1, i + 6)]), r)
                att = 1 if s == 0 else 0
                aa, ams = cur_anim.get(att, {}).get(t, (None, None))
                ar = frames.get((att, t))
                dist = (math.hypot(r["x"] - ar["x"], r["z"] - ar["z"]) if ar and None not in (r.get("x"), ar.get("x")) else None)
                hit_rows.append((t, r["ms"], s, who(s, t), dhp, dpo, r.get("hp"), r.get("posture"),
                                 anim_name(p.get("anim")), anim_name(later.get("anim")), att, anim_name(aa),
                                 (r["ms"] - ams) if ams is not None else None, dist))
    sp_rows = [(s, w, i, t0, t1, d, anim_name(cur_anim.get(s, {}).get(t0, (None, 0))[0])) for s, w, i, t0, t1, d, _ in sp_rows]

    # chr pointers / handles -> names for the DLL hook lines (rec2 "player", "slot" and "scan" events)
    by_ptr, by_handle = {}, {}
    for e in events:
        k = e.get("kind")
        if k == "player":
            by_ptr[int(e["ptr"], 16)] = "wolf"
            by_handle[e["handle"]] = "wolf"
        elif k == "slot" and e.get("ptr", "0") != "0":
            n = f"c{e.get('chr_id', -1)}:{e.get('handle', -1)}"
            by_ptr[int(e["ptr"], 16)] = n
            by_handle[e.get("handle")] = n
        elif k == "scan" and e.get("chrs"):
            for c in e["chrs"].split(","):
                ptr, hnd, cid = c.split(":")
                n = f"c{cid}:{hnd}"
                by_ptr.setdefault(int(ptr, 16), n)
                by_handle.setdefault(int(hnd), n)
    slot_of_name = {}
    for s in sorted(slot_changes):
        for t, w in slot_changes[s]:
            slot_of_name.setdefault(w.split(":", 1)[1], []).append((t, s))
    tick_ms = {t: r["ms"] for (s, t), r in frames.items() if s == 0}

    def ms_of(t):
        return tick_ms.get(t, (t or 0) * 1000.0 / 60)

    def anim_at(name, t):
        """(anim name, ms into it) of a named chr at tick t when it is Wolf or in slot 1/2."""
        if name is None:
            return None, None
        s = 0 if name == "wolf" else None
        for t0, ss in slot_of_name.get(name, []):
            if t0 <= t:
                s = ss
        if s is None or who(s, t).split(":", 1)[-1] != name and s != 0:
            return None, None
        a, ams = cur_anim.get(s, {}).get(t, (None, None))
        return anim_name(a) if a is not None else None, (ms_of(t) - ams) if ams is not None else None

    tae_rows, fx_rows, clip_rows, rag_last = [], [], [], None
    hit_last = {}
    for h in _jsonl(rec / "hooks.jsonl"):
        name = h.get("name") or h.get("hook_name") or str(h.get("hook_id", "?"))
        t = h.get("tick") or 0
        if name == "tae":
            w = by_handle.get(h.get("handle")) or by_ptr.get(h.get("chr")) or f"h:{h.get('handle')}"
            an, at = anim_at(w, t)
            tae_rows.append((t, h.get("end"), ms_of(t), ms_of(h.get("end") or t) - ms_of(t), w, h.get("type"), an, at, h.get("args")))
            continue
        if name == "vfx":
            # FXHG create: no owner pointer; the nearest of Wolf / slot 1 / 2 within 4 m of the position
            w, pos = None, None
            xf = h.get("xf") or ""
            if len(xf) >= 24:
                import struct
                pos = struct.unpack("<3f", bytes.fromhex(xf[:24]))
                best = 4.0
                for s in range(max(slot_changes, default=2) + 1):
                    r = frames.get((s, t))
                    if r and None not in (r.get("x"), r.get("y"), r.get("z")):
                        d = math.dist(pos, (r["x"], r["y"], r["z"]))
                        if d < best:
                            best, w = d, who(s, t).split(":", 1)[-1] if s else "wolf"
            an, at = anim_at(w, t)
            fx_rows.append((t, ms_of(t), name, w, h.get("ffx"), h.get("ret"), an, at,
                            None if pos is None else f"{pos[0]:.2f},{pos[1]:.2f},{pos[2]:.2f}", xf))
            continue
        if name == "sound":
            w = by_ptr.get((h.get("emitters") or 0) - 0x1B40)
            an, at = anim_at(w, t)
            fx_rows.append((t, ms_of(t), name, w, h.get("sound"), h.get("sound_type"), an, at, None, None))
            continue
        if name == "bullet":
            fx_rows.append((t, ms_of(t), name, None, h.get("bullet"), None, None, None, None, h.get("req")))
            continue
        if name == "hit":
            w = by_handle.get(h.get("target_handle")) or by_ptr.get(h.get("target"))
            att = by_ptr.get(h.get("owner"))
            # the funnel runs every frame a hitbox touches: one row per contact (same pair, gap <= 2 ticks)
            key = (h.get("owner"), h.get("target"))
            last = hit_last.get(key)
            hit_last[key] = t
            if last is not None and t - last <= 2:
                continue
            an, at = anim_at(att, t)
            fx_rows.append((t, ms_of(t), name, w, h.get("target_handle"), None, an, at, att, h.get("blob")))
            continue
        if name == "lock":
            w = by_ptr.get(h.get("target")) or (None if not h.get("target") else f"h:{h.get('target_handle')}")
            fx_rows.append((t, ms_of(t), name, w, h.get("target_handle"), None, None, None, None, None))
            continue
        if name == "emevd":
            # id = event id, sub = bank * 1000 + command, other = named command with decoded args
            fx_rows.append((t, ms_of(t), name, None, h.get("event"), h.get("bank", 0) * 1000 + h.get("cmd", 0), None, None,
                            emevd(h.get("bank"), h.get("cmd"), h.get("args")), h.get("args")))
            continue
        if name == "throw":
            fx_rows.append((t, ms_of(t), name, by_handle.get(h.get("victim")), h.get("throw"), None, *anim_at(by_handle.get(h.get("thrower")), t),
                            by_handle.get(h.get("thrower")), None))
            continue
        if name == "flag":
            fx_rows.append((t, ms_of(t), name, None, h.get("flag"), h.get("value"), None, None, None, None))
            continue
        if name in ("sp_add", "sp_del"):
            # event-driven SpEffect add / remove (catches effects shorter than a tick); other = source
            w = by_handle.get(h.get("owner")) or f"h:{h.get('owner')}"
            an, at = anim_at(w, t)
            fx_rows.append((t, ms_of(t), name, w, h.get("sp"), None, an, at,
                            by_handle.get(h.get("source")) if name == "sp_add" else None, None))
            continue
        if name == "tae920":
            w = by_handle.get(h.get("handle")) or f"h:{h.get('handle')}"
            an, at = anim_at(w, t)
            fx_rows.append((t, ms_of(t), name, w, h.get("row"), h.get("old"), an, at, None, None))
            continue
        if name in ("homing", "fall", "falldmg"):
            w = by_handle.get(h.get("handle")) or f"h:{h.get('handle')}"
            an, at = anim_at(w, t)
            if name == "homing":
                i, sub, other = h.get("mode"), h.get("ok"), json.dumps({k: h.get(k) for k in ("from", "to", "vel", "row")})
            elif name == "fall":
                i, sub, other = h.get("mtrl"), h.get("kill"), h.get("height")
            else:
                i, sub, other = None, None, json.dumps({k: h.get(k) for k in ("out", "h8fc", "p7", "p8")})
            fx_rows.append((t, ms_of(t), name, w, i, sub, an, at, other, None))
            continue
        if name == "hks":
            fx_rows.append((t, ms_of(t), name, None, None, None, None, None, h.get("event"), str(h.get("lua"))))
            continue
        if name == "hks_state":
            # behaviour-graph script state on (id 1) / off (id 0); raw = graph context (one per character)
            fx_rows.append((t, ms_of(t), name, None, 1 if h.get("kind") == "on" else 0, None, None, None, h.get("state"), str(h.get("ctx"))))
            continue
        if name == "anim":
            # every active hkb clip of Wolf this frame (rec-hooks anim.rs); ragdoll state changes go to fx
            for c in h.get("clips") or []:
                clip_rows.append((t, ms_of(t), c.get("name"), c.get("anim"), c.get("t"), c.get("w"), c.get("speed"), c.get("end"), c.get("rm")))
            rag = (h.get("rag_req"), h.get("rag_state"), h.get("rag"), h.get("pose_on"))
            if rag != rag_last:
                rag_last = rag
                fx_rows.append((t, ms_of(t), "ragdoll", "wolf", h.get("rag_state"), h.get("rag"), *anim_at("wolf", t), json.dumps(rag), None))
            continue
        if name == "bullet_pos":
            # [handle, x, y, z, metres flown, flags] per active bullet (rec-hooks anim.rs bullets)
            for b in h.get("b") or []:
                fx_rows.append((t, ms_of(t), name, None, b[0], b[5], None, b[4], f"{b[1]},{b[2]},{b[3]}", None))
            continue
        if name in ("obj_broken", "obj_destroy"):
            # map object broken / destroy request (rec-hooks): id = object handle, sub = HP (destroy: level), other = position
            fx_rows.append((t, ms_of(t), name, None, h.get("obj"), h.get("level", h.get("hp")), None, h.get("obj_state"),
                            ",".join(str(v) for v in h.get("pos") or []), json.dumps(h)))
            continue
        if name == "objact":
            # an ObjAct runs (door, lever, chest): id = area, sub = entry index, raw = ExecInfo bytes
            fx_rows.append((t, ms_of(t), name, None, h.get("area"), h.get("entry"), None, None, None, h.get("exec")))
            continue
        if name in ("fmod_param", "fmod_param_i", "fmod_global"):
            # FMOD Ex parameter write, logged on a change: other = parameter (name or index), at = value, raw = cue
            fx_rows.append((t, ms_of(t), name, None, None, None, None, h.get("value"), str(h.get("param")), str(h.get("cue"))))
            continue
        if name == "noise":
            # a noise for enemy hearing: other = source info bytes, raw = emit info bytes
            fx_rows.append((t, ms_of(t), name, None, None, None, None, None, h.get("source"), h.get("emit")))
            continue
        if name == "stick":
            # the pad's input frame before any deadzone (logged on change): at = forward/back, other = "side,look x,look y"
            look = h.get("look") or [None, None]
            fx_rows.append((t, ms_of(t), name, "wolf", None, None, None, h.get("fwd"), f"{h.get('side')},{look[0]},{look[1]}", None))
            continue
        if name == "play":
            # scripted input (rec-hooks play.rs): start / end of an input script
            fx_rows.append((t, ms_of(t), name, "wolf", h.get("frames"), None, *anim_at("wolf", t), h.get("play"), None))
            continue
        db.execute("INSERT INTO hooks VALUES(?,?,?,?,?,?,?,?)", (h.get("tick"), name, h.get("arg1"), h.get("arg2"), h.get("arg3"),
                   h.get("item"), h.get("count"), json.dumps(h)))
    db.executemany(f"INSERT INTO tae VALUES({','.join('?' * 9)})", tae_rows)
    db.executemany(f"INSERT INTO fx VALUES({','.join('?' * 10)})", fx_rows)
    db.executemany(f"INSERT INTO clips VALUES({','.join('?' * 9)})", clip_rows)
    db.executemany("INSERT INTO events VALUES(?,?,?,?,?,?,?,?)", ev_rows)
    db.executemany(f"INSERT INTO anims VALUES({','.join('?' * 15)})", an_rows)
    db.executemany(f"INSERT INTO hits VALUES({','.join('?' * 14)})", hit_rows)
    db.executemany("INSERT INTO speffects VALUES(?,?,?,?,?,?,?)", sp_rows)
    db.execute("INSERT OR REPLACE INTO meta VALUES('end_tick', ?)", (str(end_tick),))
    db.execute("INSERT OR REPLACE INTO meta VALUES('schema', ?)", (SCHEMA_VERSION,))
    db.commit()
    db.close()
    return db_path


def _segment(seg, last, prev_name, next_name):
    f = seg["first"]
    dur = last["ms"] - f["ms"]
    dist = speed = turn = dy = None
    if None not in (f.get("x"), last.get("x"), f.get("z"), last.get("z")):
        dist = math.hypot(last["x"] - f["x"], last["z"] - f["z"])
        speed = dist / (dur / 1000) if dur > 0 else None
        dy = last["y"] - f["y"]
    if f.get("yaw") is not None and last.get("yaw") is not None:
        d = last["yaw"] - f["yaw"]
        turn = math.degrees((d + math.pi) % (2 * math.pi) - math.pi)
    lock = 1 if f.get("lock_flag") else 0
    return (seg["slot"], seg["who"], seg["anim"], anim_name(seg["anim"]), seg["t0"], last["tick"], f["ms"], dur, dist, speed,
            turn, dy, prev_name, next_name, lock)
