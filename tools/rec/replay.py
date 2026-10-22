"""Replay a recording's inputs in our sim and diff the animation timeline against the real game.

  python -m tools.rec replay <rec> [--from T] [--to T] [--limit N]

1. Writes <rec>/replay_inputs.csv: Wolf's recorded stick and buttons per tick, turned into our sim's frame
   (the game mirrors X: our x = -Sekiro x. Fitted on the movement recording's velocity: Sekiro's move vector is world
   (move_x, move_z) and its facing is (-sin yaw, -cos yaw), so our yaw = -Sekiro yaw; at camera yaw 0 our stick is
   world (x, -z), so stick = (-move_x, -move_z)). The recorder reads the inputs one tick after the HKS script used
   them (every button / stick edge lands 1 tick after its W_ event), so row t carries tick t + INPUT_LEAD's inputs.
2. Runs the ignored sim test `replay_recording` (src/sim_tests/part17.rs) on it -> <rec>/replay_ours.csv.
3. Prints a short diff: the HKS events (W_...) the game fired (hkbFireEvent hook) against the ones our script fired,
   matched by name within EVENT_WINDOW ticks; then tick-level anim agreement, per-anim durations, diverging stretches.
   The recorded anim (TimeAct +0x100) is the last TimeAct the game STARTED: loops and chains the behaviour graph plays
   itself (SprintLoop, the jump start) never show there, so the events are the reliable side of old recordings.
The sim has no map: walls, slopes and enemies the real Wolf met are not there, so long runs drift.
"""
import csv
import json
import math
import os
import re
import sqlite3
import subprocess
from collections import defaultdict
from pathlib import Path

from .fields import FACE_LAYER, anim_name
from .ingest import ingest

BUTTONS = ["btn_attack", "btn_guard", "btn_step", "btn_jump", "btn_crouch", "btn_item", "btn_prosthetic", "btn_art"]
REPO = Path(__file__).resolve().parents[2]
EVENT_WINDOW = 20
INPUT_LEAD = 1
# events only Wolf's c0000 script fires: pick his Lua state among all characters' hkbFireEvent calls
WOLF_EVENTS = {"W_StandMoveStart", "W_GroundStep", "W_SprintStartFromStep", "W_GroundJumpReady", "W_StandRunStop"}


def wolf_events(db):
    """tick -> [event] of Wolf's script (fx kind 'hks', raw = Lua state)."""
    rows = db.execute("SELECT tick, other, raw FROM fx WHERE kind = 'hks' ORDER BY tick").fetchall()
    by_lua = defaultdict(list)
    for t, ev, lua in rows:
        by_lua[lua].append((t, ev))
    # a death or a load gives Wolf a new Lua state: every state that fires his own events is his
    wolf = [l for l, evs in by_lua.items() if any(e in WOLF_EVENTS for _, e in evs)]
    out = defaultdict(list)
    for l in wolf:
        for t, ev in by_lua[l]:
            out[t].append(ev)
    return out


def art_rows():
    """The combat art rows of EquipParamWeapon (5000-9999 with a spAtkcategory): id -> row."""
    path = REPO / "extracted" / "json" / "params" / "EquipParamWeapon.json"
    try:
        d = json.load(open(path, encoding="utf-8"))
    except OSError:
        return {}
    rows = d.get("rows", d) if isinstance(d, dict) else d
    pairs = rows.items() if isinstance(rows, dict) else ((r.get("ID", r.get("id")), r) for r in rows)
    return {int(k): r for k, r in pairs if isinstance(r, dict) and r.get("spAtkcategory") and str(k).isdigit() and 5000 <= int(k) < 10000}


def art_ids():
    """Combat art clip group a1NN -> its EquipParamWeapon art (the first row with spAtkcategory 1NN)."""
    out = {}
    for k, r in sorted(art_rows().items()):
        out.setdefault(int(r["spAtkcategory"]), k)
    return out


def by_resident(db, res):
    """What Wolf had equipped by tick, from the resident SpEffects (resident -> id) the recording
    logged: (start tick, id) in order."""
    if not res:
        return []
    try:
        q = f"SELECT start_tick, id FROM speffects WHERE slot = 0 AND id IN ({','.join(map(str, res))}) ORDER BY start_tick"
        return [(t, res[i]) for t, i in db.execute(q)]
    except sqlite3.Error:
        return []


def art_by_resident(db):
    """The combat art by tick: its row's resident SpEffect (the upgraded rows' carry the unlock
    refs 280-287)."""
    return by_resident(db, {int(r["residentSpEffectId"]): k for k, r in sorted(art_rows().items()) if int(r.get("residentSpEffectId") or -1) > 0})


def tool_by_resident(db):
    """The prosthetic by tick: only the active tool's resident is on (127000-127920)."""
    try:
        tools = json.load(open(REPO / "extracted" / "combat_data.json", encoding="utf-8"))["player"]["prosthetics"]
    except (OSError, KeyError):
        return []
    return by_resident(db, {t["resident"]: t["id"] for t in tools if t.get("resident", 0) > 0})


def hit_stop_scales(db):
    """Per tick of a hit Wolf landed (hits, attacker slot 0): the victim's hit stop scale on Wolf,
    NpcParam hitStopType (0 none, 1 half, 2 full; combat.rs start_hit_stop), the most common
    type over its chr's rows (c5100 -> 5100xxxx). The replay keeps the enemy out of reach, so it
    starts Wolf's hit stop itself at these ticks."""
    path = REPO / "extracted" / "json" / "params" / "NpcParam.json"
    try:
        d = json.load(open(path, encoding="utf-8"))
    except OSError:
        return {}
    rows = d.get("rows", d) if isinstance(d, dict) else d
    pairs = rows.items() if isinstance(rows, dict) else ((r.get("ID", r.get("id")), r) for r in rows)
    types = defaultdict(lambda: defaultdict(int))
    for k, r in pairs:
        if isinstance(r, dict) and str(k).isdigit():
            types[int(k) // 10000][r.get("hitStopType") or 0] += 1
    scale = {0: 0.0, 1: 0.5, 2: 1.0}
    out = {}
    for tick, victim in db.execute("SELECT tick, victim FROM hits WHERE attacker_slot = 0"):
        m = re.match(r"\d+:c(\d{4})", victim or "")
        if m and types.get(int(m[1])):
            t = types[int(m[1])]
            out[tick] = max(out.get(tick, 0.0), scale.get(max(t, key=t.get), 0.0))
    return out


LOCK_SWITCH_TICKS = 30


def lock_targets(db):
    """Per tick: the locked enemy's offset from Wolf (game frame; the recorder's lock_target is
    unread, -1). Slots 1-2 are the nearest two enemies and swap places, so the target is followed
    by its id: at the lock's start the one the camera looks at most squarely, kept while it is
    recorded (held where last seen while a nearer enemy fills the slots), switched only when the
    camera favours the other for LOCK_SWITCH_TICKS."""
    cam = {r[0]: r[1:] for r in db.execute("SELECT tick, cam_x, cam_z, cam_fwd_x, cam_fwd_z FROM globals")}
    at, locked = defaultdict(dict), {}
    for t, s, who, x, z, lock in db.execute("SELECT tick, slot, who, x, z, lock_flag FROM frames"):
        if x is None:
            continue
        if s == 0:
            at[t][0] = (x, z)
            locked[t] = bool(lock)
        else:
            at[t][who] = (x, z)
    out, cur, doubt, last = {}, None, 0, {}
    for t in sorted(at):
        seen, c = at[t], cam.get(t)
        w = seen.get(0)
        if not locked.get(t) or w is None or c is None or None in c:
            cur, doubt = None, 0
            continue
        cx, cz, fx, fz = c
        def square(p):
            d = math.hypot(p[0] - cx, p[1] - cz)
            return -2.0 if d < 1e-3 else ((p[0] - cx) * fx + (p[1] - cz) * fz) / d
        best = max((k for k in seen if k != 0), key=lambda k: square(seen[k]), default=None)
        if cur is None:
            cur, doubt = best, 0
        elif cur not in seen:
            # out of the two recorded slots (a nearer enemy came in): where it was last seen
            if cur in last:
                out[t] = (last[cur][0] - w[0], last[cur][1] - w[1])
            continue
        elif best != cur and square(seen[best]) > square(seen[cur]) + 0.05:
            doubt += 1
            if doubt >= LOCK_SWITCH_TICKS:
                cur, doubt = best, 0
        else:
            doubt = 0
        if cur is not None:
            last[cur] = seen[cur]
            out[t] = (seen[cur][0] - w[0], seen[cur][1] - w[1])
    return out


def timeline(events, ticks):
    """tick -> the last (start tick, id) value at or before it (None before the first)."""
    out, i, cur = {}, 0, None
    for t in ticks:
        while i < len(events) and events[i][0] <= t:
            cur = events[i][1]
            i += 1
        out[t] = cur
    return out


def export(rec, t0=None, t1=None, name="replay_inputs.csv"):
    db = sqlite3.connect(ingest(rec))
    q = "SELECT tick, anim, x, z, yaw, move_x, move_z, lock_flag, " + ", ".join(BUTTONS) + " FROM frames WHERE slot = 0"
    rows = [r for r in db.execute(q + " ORDER BY tick") if (t0 is None or r[0] >= t0) and (t1 is None or r[0] <= t1)]
    if not rows:
        raise SystemExit("no Wolf frames in that range")
    x0, z0 = rows[0][2] or 0.0, rows[0][3] or 0.0
    events = wolf_events(db)
    out = Path(rec) / name
    last_anim = None
    inputs = {r[0]: r for r in rows}
    # the combat art Wolf had: its resident SpEffect's (art_by_resident), else the next art clip
    # group he plays (a1NN_; the art is swapped in the menu before its first use), else the last
    arts, art, held = art_ids(), None, {}
    for r in reversed(rows):
        anim = r[1]
        if isinstance(anim, int) and 100 <= anim // 1_000_000 <= 110:
            art = arts.get(anim // 1_000_000, art)
        held[r[0]] = art
    for t, a in timeline(art_by_resident(db), [r[0] for r in rows]).items():
        if a is not None:
            held[t] = a
    tools = timeline(tool_by_resident(db), [r[0] for r in rows])
    targets = lock_targets(db)
    stops = hit_stop_scales(db)
    art = None
    with open(out, "w", newline="", encoding="utf-8") as fh:
        w = csv.writer(fh)
        w.writerow(["tick", "stick_x", "stick_y", *[b[4:] for b in BUTTONS], "lock", "anim", "x", "z", "yaw", "events", "art_id", "tool_id", "lock_dx", "lock_dz", "hit_stop"])
        for r in rows:
            tick, anim, x, z, yaw = r[:5]
            inp = inputs.get(tick + INPUT_LEAD, r)
            mx, mz, lock = inp[5:8]
            if anim is not None and anim not in FACE_LAYER:
                last_anim = anim
            art = held[tick] or art  # after the last art clip: that art
            w.writerow([tick, -(mx or 0.0), -(mz or 0.0), *[int(bool(b)) for b in inp[8:]], int(bool(lock)),
                        anim_name(last_anim) if last_anim is not None else "",
                        None if x is None else -(x - x0), None if z is None else z - z0,
                        None if yaw is None else -yaw, ";".join(events.get(tick, [])), art or "", tools[tick] or "",
                        *(lambda d: ("", "") if d is None or not lock else (round(-d[0], 3), round(d[1], 3)))(targets.get(tick + INPUT_LEAD)),
                        stops.get(tick, "")])
    return out


def run_sim(inputs, out, extra_env=None):
    env = dict(os.environ, SHINOBI_REPLAY=str(inputs), SHINOBI_REPLAY_OUT=str(out), **(extra_env or {}))
    env.setdefault("CARGO_TARGET_DIR", str(REPO / "target" / "bevy20"))
    r = subprocess.run(["cargo", "test", "--profile", "quick", "--lib", "replay_recording", "--", "--ignored", "--nocapture"],
                       cwd=REPO, env=env, capture_output=True, text=True)
    if r.returncode != 0 or not Path(out).exists():
        raise SystemExit("sim replay failed:\n" + (r.stdout + r.stderr)[-3000:])


def _segments(labels):
    segs, cur = [], None
    for t, a in labels:
        if cur is None or a != cur[0]:
            if cur:
                segs.append(cur)
            cur = [a, t, t]
        else:
            cur[2] = t
    if cur:
        segs.append(cur)
    return segs


def diff(inputs, ours, limit=12):
    real, sim, _ = _load(inputs, ours)
    if sim:
        s0 = sim[min(sim)]
        ox, oz = float(s0["x"]), float(s0["z"])
        for r in sim.values():
            r["x"], r["z"] = str(float(r["x"]) - ox), str(float(r["z"]) - oz)
    ticks = sorted(set(real) & set(sim))
    L = _event_diff(real, sim, ticks, limit)
    same = sum(1 for t in ticks if real[t]["anim"] == sim[t]["anim"])
    L += ["", f"{len(ticks)} ticks, same recorded anim on {same} ({100 * same / max(1, len(ticks)):.0f} %; see the note on loops)"]
    # per anim: median duration real vs ours
    dur = defaultdict(lambda: ([], []))
    for i, src in ((0, real), (1, sim)):
        for a, s, e in _segments([(t, src[t]["anim"]) for t in ticks]):
            dur[a][i].append((e - s + 1) / 60)
    med = lambda v: sorted(v)[len(v) // 2] if v else None
    rows = sorted(dur.items(), key=lambda kv: -len(kv[1][0]))
    L += ["", "anim | n real | n ours | median s real | median s ours"]
    fs = lambda v: f"{med(v):.2f}" if v else "-"
    for a, (r, o) in rows[:limit * 2]:
        L.append(f"{a} | {len(r)} | {len(o)} | {fs(r)} | {fs(o)}")
    # first diverging stretches (>= 6 ticks of a different anim)
    L += ["", "diverging stretches (tick: real -> ours, ours state, position error m)"]
    run, n = None, 0
    for t in ticks:
        bad = real[t]["anim"] != sim[t]["anim"]
        if bad and run is None:
            run = t
        if not bad and run is not None:
            if t - run >= 6 and n < limit:
                r, o = real[run], sim[run]
                err = None
                if r["x"] and o["x"]:
                    err = math.hypot(float(r["x"]) - float(o["x"]), float(r["z"]) - float(o["z"]))
                L.append(f"{run}..{t - 1}: {r['anim']} -> {o['anim']} ({o['state']}), err {err:.2f}" if err is not None else
                         f"{run}..{t - 1}: {r['anim']} -> {o['anim']} ({o['state']})")
                n += 1
            run = None
    return L


# Real events that need a world the sim does not have (enemies' hits, water, walls, wire points,
# NPCs, map events): the stretch around them is left out of the match, as are F4 teleports.
WORLD = re.compile(r"Damage|Down|Swim|Dive|Throw|Deathblow|Deflect(Hard|Easy|Normal|Weak)|AddHardDeflect|GuardBreak|"
                   r"Deflected|DeflectBreak|EnemyJump|Positioning|Hang|Wall|Ladder|Docking|Wire|Talk|Event|Death|Blow|Kawarimi|Water|Ride|Eavesdrop|Cover")
MASK_BEFORE, MASK_AFTER = 10, 90
# A throw's or deathblow's clip starts 2-3 ticks after the attack press, a boss's last deathblow
# (its walk-up first) 18-30 (the recordings' press -> a2xx leads): masked from that press.
THROW_BEFORE = 30


def _press_before(real, t):
    """How far before t the attack press that started a throw's clip came (MASK_BEFORE at least)."""
    for back in range(1, THROW_BEFORE + 1):
        if real.get(t - back, {}).get("attack") == "1" and real.get(t - back - 1, {}).get("attack") != "1":
            return max(back, MASK_BEFORE)
    return MASK_BEFORE


STALL_EVENTS = 4


RESYNC_MAX, RESYNC_SLACK = 900, 2


def _clip_starts(rows, ticks):
    """Per tick: the tick its anim's run started (the clip's own start)."""
    out, start, last = {}, None, None
    for t in ticks:
        a = rows.get(t, {}).get("anim")
        if a != last:
            start, last = t, a
        out[t] = start
    return out


def masked_ticks(real, ticks, sim=None):
    """Ticks left out of the event match: around world events, stalls, teleports and the range's ends.
    With our side given, a world event's stretch runs on until both play the same clip, started
    within RESYNC_SLACK ticks of each other (a hit the sim never took leaves it out of step for as
    long as its chain of moves lasts)."""
    out = set(ticks[:10]) | set(ticks[-EVENT_WINDOW:])
    starts = (_clip_starts(real, ticks), _clip_starts(sim, ticks)) if sim else None
    index = {t: i for i, t in enumerate(ticks)}
    prev = None
    for t in ticks:
        r = real[t]
        evs = [e for e in (r.get("events") or "").split(";") if e and not e.startswith("W_AddBlend")]
        # a stall: the game ran frames the recorder did not tick, their events land on one tick
        hit = any(WORLD.search(e) for e in evs) or len(evs) >= STALL_EVENTS
        # a throw's or deathblow's clip (ThrowParam groups a2xx): the exe starts those, no W_ event
        throw = (r.get("anim") or "")[1:2] == "2" and (prev or {}).get("anim", "")[1:2] != "2"
        hit |= (r.get("anim") or "")[1:2] == "2"
        if r.get("x") and prev is not None and prev.get("x"):
            hit |= math.hypot(float(r["x"]) - float(prev["x"]), float(r["z"]) - float(prev["z"])) > 2.0
        if hit:
            end = t + MASK_AFTER
            if starts:
                i = index.get(end)
                while i is not None and i < len(ticks) and ticks[i] - end < RESYNC_MAX:
                    u = ticks[i]
                    if real[u].get("anim") == sim.get(u, {}).get("anim") and abs(starts[0][u] - starts[1][u]) <= RESYNC_SLACK:
                        break
                    i += 1
                end = ticks[i] if i is not None and i < len(ticks) else end
            out.update(range(t - (_press_before(real, t) if throw else MASK_BEFORE), end + 1))
        prev = r
    return out


def match_events(real, sim, ticks):
    """Pairs each game event with the nearest same-named one of ours within EVENT_WINDOW ticks."""
    mask = masked_ticks(real, ticks, sim)
    # (the face and speech layers, W_AddBlend*, are not compared)
    evs = lambda src: [(t, e) for t in ticks if t not in mask for e in (src[t].get("events") or "").split(";") if e and not e.startswith("W_AddBlend")]
    re_, oe = evs(real), evs(sim)
    used, matched, missing = set(), defaultdict(list), []
    for t, e in re_:
        best = None
        for i, (t2, e2) in enumerate(oe):
            if i in used or e2 != e or abs(t2 - t) > EVENT_WINDOW:
                continue
            if best is None or abs(t2 - t) < abs(oe[best][0] - t):
                best = i
        if best is None:
            missing.append((t, e))
        else:
            used.add(best)
            matched[e].append(oe[best][0] - t)
    extra = [oe[i] for i in range(len(oe)) if i not in used]
    return {"game": re_, "ours": oe, "matched": matched, "missing": missing, "extra": extra,
            "masked": len(mask & set(ticks)), "ticks": len(ticks)}


def _event_diff(real, sim, ticks, limit):
    m = match_events(real, sim, ticks)
    n_real, n_ours = defaultdict(int), defaultdict(int)
    for _, e in m["game"]:
        n_real[e] += 1
    for _, e in m["ours"]:
        n_ours[e] += 1
    nm = sum(len(v) for v in m["matched"].values())
    L = [f"HKS events: game {len(m['game'])}, ours {len(m['ours'])}, matched {nm} within {EVENT_WINDOW} ticks "
         f"({m['masked']} of {m['ticks']} ticks left out: enemy hits, water, walls, teleports, ...)",
         "event | game | ours | matched | median ticks ours - game"]
    for e in sorted(set(n_real) | set(n_ours), key=lambda e: -(n_real[e] + n_ours[e]))[:limit * 2]:
        d = sorted(m["matched"][e])
        L.append(f"{e} | {n_real[e]} | {n_ours[e]} | {len(d)} | {d[len(d) // 2] if d else '-'}")
    L += ["", "game events ours missed (tick event): " + ", ".join(f"{t} {e}" for t, e in m["missing"][:limit])]
    L += ["ours only (tick event): " + ", ".join(f"{t} {e}" for t, e in m["extra"][:limit])]
    return L


def _load(inputs, ours):
    real = {int(r["tick"]): r for r in csv.DictReader(open(inputs, encoding="utf-8"))}
    sim = {int(r["tick"]): r for r in csv.DictReader(open(ours, encoding="utf-8"))}
    return real, sim, sorted(set(real) & set(sim))


def replay_all(logs, limit=25, cached=False):
    """Every recording with Wolf's events: replay, then one table of the events that differ most.
    cached: reuse each recording's replay_inputs.csv / replay_ours.csv (no sim runs)."""
    tot = defaultdict(lambda: [0, 0, 0, []])  # game, ours, matched, (rec, tick, kind) samples
    done, skipped = [], []
    for d in sorted(Path(logs).glob("rec_*")):
        if not (d / "frames.bin").exists():
            continue
        try:
            inputs, ours = d / "replay_inputs.csv", d / "replay_ours.csv"
            if not cached:
                inputs = export(d)
                run_sim(inputs, ours)
            elif not ours.exists():
                continue
        except (SystemExit, Exception) as e:  # old recorder versions lack the button columns
            skipped.append(f"{d.name}: {str(e).splitlines()[0][:80] if str(e) else type(e).__name__}")
            continue
        m = match_events(*_load(inputs, ours))
        if not m["game"]:
            skipped.append(f"{d.name}: no Wolf events")
            continue
        nm = sum(len(v) for v in m["matched"].values())
        done.append(f"{d.name}: game {len(m['game'])}, ours {len(m['ours'])}, matched {nm}")
        for _, e in m["game"]:
            tot[e][0] += 1
        for _, e in m["ours"]:
            tot[e][1] += 1
        for e, v in m["matched"].items():
            tot[e][2] += len(v)
        for t, e in m["missing"]:
            tot[e][3].append(f"{d.name[4:19]}@{t} missed")
        for t, e in m["extra"]:
            tot[e][3].append(f"{d.name[4:19]}@{t} extra")
    off = lambda v: (v[0] - v[2]) + (v[1] - v[2])
    L = [f"{len(done)} recordings replayed, {len(skipped)} skipped", *done, "",
         "event | game | ours | matched | unmatched | samples"]
    for e, v in sorted(tot.items(), key=lambda kv: -off(kv[1]))[:limit]:
        L.append(f"{e} | {v[0]} | {v[1]} | {v[2]} | {off(v)} | {', '.join(v[3][:4])}")
    L += ["", "skipped: " + "; ".join(skipped)]
    return L


def replay(rec, t0=None, t1=None, limit=12):
    inputs = export(rec, t0, t1)
    ours = Path(rec) / "replay_ours.csv"
    run_sim(inputs, ours)
    return diff(inputs, ours, limit)
