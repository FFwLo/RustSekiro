"""Enemies and bosses against the real game's recordings: which attacks each one starts, from how
far, how often, and how it takes Wolf's hits, in the game and in our sim on the same fight.

Our side replays the fight (enemy mode of the ignored test replay_recording): Wolf plays the
recorded inputs, the enemy fights with its AI on from the recorded start offset, and neither dies.
The AI rolls its own dice, so the two are compared as profiles, not tick by tick.

  python -m tools.rec enemies <rec> [--chr cXXXX]   one recording (its busiest enemies, or one kind)
  python -m tools.rec enemies all [--cached]         every recording, per kind, to logs/enemies_all.md
"""
import csv
import re
import sqlite3
from collections import Counter, defaultdict
from pathlib import Path
from statistics import median

from .ingest import ingest
from .replay import export, run_sim

ATTACK = re.compile(r"^a\d{3}_003\d{3}$")
# the clip group's thousands (a000_00X...): what kind of anim the enemy went into after a hit
CLASS = {"3": "attack", "9": "damage", "0": "idle/move", "1": "move", "2": "event", "4": "move", "5": "move", "6": "move", "7": "throw"}
MIN_ATTACKS = 3
# the game's hit record reads the victim's anim 6 ticks after the hit (ingest: hits.reaction); ours
# the same, or a quick counter (Isshin's 3071 out of 8400 at 0.1 s) reads as "kept on"
REACT_AFTER = 6
# Wolf's distance bands: the attack a script picks hangs on the distance, and the replayed Wolf does
# not walk where the game's did, so attacks are compared per band (rate while in it, and the picks).
BANDS = [(0, 3), (3, 6), (6, 10), (10, 20), (20, 999)]


def _band(d):
    return next(i for i, (lo, hi) in enumerate(BANDS) if lo <= d < hi)


def _group(name):
    """'a100_003007' -> 100: the anim group (a boss's next phase plays its own, e.g. the Headless Ape a100)."""
    m = re.match(r"^a(\d{3})_", name or "")
    return int(m[1]) if m else None


def _anim_class(name):
    m = re.match(r"^a\d{3}_00(\d)(\d)\d{2}$", name or "")
    if not m:
        return "other"
    # (c7100's state names) 80xx-83xx damage / collapse, 84xx JustGuardDamage (it deflected),
    # 85xx GuardDamage / GuardBreak, 86xx-87xx AttackBound (its attack repelled), 95xx SABlend
    # (hit in hyper armour), 96xx AttackJustGuardBound
    if m[1] == "8":
        return {"4": "deflect", "5": "guard", "6": "repelled", "7": "repelled"}.get(m[2], "damage")
    if m[1] == "9":
        return {"5": "hyper armour", "6": "repelled"}.get(m[2], "damage")
    return CLASS.get(m[1], "other")


def _entity(who):
    """'1:c51000000:268583084' -> ('c5100', 'c51000000:268583084'): the slot number left out."""
    m = re.match(r"^\d+:(c(\d{4})\d*:\d+)$", who or "")
    return (f"c{m[2]}", m[1]) if m else (None, None)


def engaged(db, chr_=None):
    """The enemies that fought in this recording (MIN_ATTACKS or more attack starts), busiest first."""
    n = Counter()
    for who, name in db.execute("SELECT who, name FROM anims WHERE slot > 0"):
        c, ent = _entity(who)
        if ent and ATTACK.match(name or "") and (chr_ is None or c == chr_):
            n[(c, ent)] += 1
    return [k for k, v in n.most_common() if v >= MIN_ATTACKS]


def _pos(db, ent):
    """Per tick: (wolf x, z), (enemy x, z), enemy anim name."""
    wolf, foe = {}, {}
    for t, slot, who, x, z, anim in db.execute("SELECT tick, slot, who, x, z, anim FROM frames"):
        if x is None:
            continue
        if slot == 0:
            wolf[t] = (x, z)
        elif _entity(who)[1] == ent:
            foe[t] = (x, z, anim, slot)
    return wolf, foe


def _moved(foe, t, slot, name):
    """The recorder logs the running anim again when the enemy changes slot (1 <-> 2, nearest
    first): not a new start if the tick before it had the same anim in the other slot."""
    prev = next((foe[u] for u in range(t - 1, t - 61, -1) if u in foe), None)
    return prev is not None and prev[3] != slot and _name(prev[2]) == name


def _dist(a, b):
    return ((a[0] - b[0]) ** 2 + (a[1] - b[1]) ** 2) ** 0.5


def game_profile(db, ent):
    wolf, foe = _pos(db, ent)
    attacks = []
    for who, name, t, slot in db.execute("SELECT who, name, start_tick, slot FROM anims WHERE slot > 0 ORDER BY start_tick"):
        if _entity(who)[1] == ent and ATTACK.match(name or "") and t in wolf and t in foe and not _moved(foe, t, slot, name):
            if not attacks or attacks[-1][1] != t:
                attacks.append((name, t, _dist(wolf[t], foe[t])))
    names = {}
    for anim in {v[2] for v in foe.values()}:
        names[anim] = _name(anim)
    hits = []
    for who, t, hp in db.execute("SELECT victim, tick, hp_delta FROM hits WHERE attacker_slot = 0"):
        if _entity(who)[1] == ent:
            after = foe.get(t + REACT_AFTER)
            hits.append((hp is not None and hp < 0, _anim_class(names.get(after[2]) if after else None), t))
    ticks = sorted(foe)
    return {"secs": len(foe) / 60.0, "attacks": attacks, "hits": hits, "t0": ticks[0] if ticks else None, "t1": ticks[-1] if ticks else None,
            "time": _time([(names.get(foe[t][2]), _dist(wolf[t], foe[t])) for t in ticks if t in wolf])}


def _time(ticks):
    """Seconds per (anim group, band), the group carried over ticks with no anim."""
    out, g = Counter(), 0
    for name, d in ticks:
        # (a000_009999 comes between a later phase's anims too: a blank, not the phase)
        g = _group(name) if _group(name) is not None and not name.endswith("_009999") else g
        out[(g, _band(d))] += 1 / 60.0
    return out


def _name(anim):
    if anim is None:
        return None
    return f"a{anim // 1_000_000:03d}_{anim % 1_000_000:06d}"


def ours_profile(path):
    rows = list(csv.DictReader(open(path, encoding="utf-8")))
    attacks, hits, last, ticks = [], [], None, []
    for i, r in enumerate(rows):
        a = r.get("e_anim") or ""
        ticks.append((a, _dist((float(r["x"]), float(r["z"])), (float(r["e_x"]), float(r["e_z"])))))
        if a != last and ATTACK.match(a):
            d = _dist((float(r["x"]), float(r["z"])), (float(r["e_x"]), float(r["e_z"])))
            attacks.append((a, int(r["tick"]), d))
        last = a
        lost, gain = float(r.get("e_hp_lost") or 0), float(r.get("e_posture_gain") or 0)
        if (lost > 0 or gain > 0) and not (hits and int(r["tick"]) - hits[-1][2] <= 3):
            after = rows[min(i + REACT_AFTER, len(rows) - 1)].get("e_anim")
            hits.append((lost > 0, _anim_class(after), int(r["tick"])))
    return {"secs": len(rows) / 60.0, "attacks": attacks, "hits": hits, "time": _time(ticks)}


def run_ours(rec, chr_, ent, prof, db):
    """Our sim on the same fight: the inputs from the enemy's first tick to its last."""
    t0, t1 = prof["t0"], prof["t1"]
    wolf, foe = _pos(db, ent)
    start = next((t for t in range(t0, t1 + 1) if t in wolf and t in foe), t0)
    w, f = wolf[start], foe[start]
    # the export's frame: x mirrored, z as is
    dx, dz = -(f[0] - w[0]), f[1] - w[1]
    tag = ent.split(":")[0]
    inputs = export(rec, start, t1, name=f"enemy_inputs_{tag}.csv")
    out = Path(rec) / f"enemy_ours_{tag}.csv"
    # its NpcParam row: the entity's name (c5100_0000 -> 51000000, the Headless Ape's 51000100)
    row = ent.split(":")[0][1:]
    # the game's Todomes on it (a boss's next phase): ours takes one at the same tick, then stands
    # where the game's stood 120 ticks on (part17 TODOME_TICKS)
    todome = []
    for who, name, t in db.execute("SELECT who, name, start_tick FROM anims WHERE slot > 0 ORDER BY start_tick"):
        if _entity(who)[1] == ent and re.match(r"^a\d{3}_0137\d\d$", name or "") and start < t <= t1:
            after = next((u for u in range(t + 120, t + 600) if u in wolf and u in foe), None)
            if after is not None:
                todome.append(f"{t}:{-(foe[after][0] - wolf[after][0]):.3f}:{foe[after][1] - wolf[after][1]:.3f}")
    # the SpEffects the game's map events put on it (an id found in an EMEVD; the replay runs none),
    # e.g. the tutorial ronin's lesson 3100xx that pick its acts
    sp = []
    for who, sid, a, b in db.execute("SELECT who, id, start_tick, end_tick FROM speffects WHERE slot > 0 ORDER BY start_tick"):
        if _entity(who)[1] == ent and (b is None or b > start) and a < t1 and _in_emevd(sid):
            sp.append(f"{sid}:{a}:{t1 + 1 if b is None else b}")
    # its anim set as the game's was at the start (20003g), until the game's changes: a map event
    # may have set it before the recording began (the tutorial ronin's 200030, m11_00 11105275,
    # over its NpcParam resident 200034)
    groups = [(t, _group(_name(foe[t][2]))) for t in sorted(foe) if start <= t <= t1 and foe[t][2] and foe[t][2] % 1_000_000 != 9999]
    if groups:
        g0 = groups[0][1]
        until = next((t for t, g in groups if g != g0), t1 + 1)
        sp.insert(0, f"{200030 + g0 // 100}:{start}:{until}")
    run_sim(inputs, out, {"SHINOBI_REPLAY_ENEMY": chr_, "SHINOBI_REPLAY_ENEMY_ROW": row, "SHINOBI_REPLAY_ENEMY_AT": f"{dx:.3f},{dz:.3f}",
                          "SHINOBI_REPLAY_ENEMY_TODOME": ";".join(todome), "SHINOBI_REPLAY_ENEMY_SP": ";".join(sp)})
    return out


# An attack started within this many ticks after one of Wolf's hits on it (guard counters, Kengeki
# answers, interrupts) answers Wolf; the replayed Wolf lands far fewer, so these are counted apart.
PROMPT = 45


def _unprompted(p):
    hit = sorted(h[2] for h in p["hits"])
    import bisect
    out = []
    for a in p["attacks"]:
        i = bisect.bisect_right(hit, a[1])
        if not (i and a[1] - hit[i - 1] <= PROMPT):
            out.append(a)
    return out


_EMEVD = None


def _in_emevd(sid):
    """Whether a map event sets the id on a character (EMEVD 2004[8] SetSpEffect with it as a
    literal), less the AI state ids 200000-200029 our stealth sets itself (20003x, the anim sets,
    stay: the tutorial ronin's 200030 over its resident 200034)."""
    global _EMEVD
    if _EMEVD is None:
        import sys
        root = Path(__file__).resolve().parents[2]
        sys.path.insert(0, str(root / "tools"))
        from boss_events import emevd_events, ints
        _EMEVD = set()
        for f in (root / "extracted" / "event").glob("*.emevd"):
            for ins, _ in emevd_events(f).values():
                for bank, i, args in ins:
                    v = ints(args)
                    if bank == 2004 and i == 8 and len(v) >= 2:
                        _EMEVD.add(v[1])
    return sid in _EMEVD and not 200000 <= sid < 200030


def _rate(p, attacks=None):
    n = len(p["attacks"] if attacks is None else attacks)
    return n / (p["secs"] / 60.0) if p["secs"] > 0 else 0.0


def _hit_line(p):
    h = p["hits"]
    if not h:
        return "no hits taken"
    hp = sum(1 for x in h if x[0])
    cls = Counter(x[1] for x in h)
    return f"{len(h)} hits taken, {100 * hp / len(h):.0f}% for HP; then " + ", ".join(f"{k} {100 * v / len(h):.0f}%" for k, v in cls.most_common(4))


def compare(chr_, game, ours, title):
    """One kind's report: game against ours."""
    L = [f"## {chr_} - {title}", ""]
    L.append(f"- time: game {game['secs']:.0f} s, ours {ours['secs']:.0f} s")
    L.append(f"- attacks / min: game {_rate(game):.1f}, ours {_rate(ours):.1f}; unprompted (no hit of Wolf's on it in the {PROMPT} ticks before): game {_rate(game, _unprompted(game)):.1f}, ours {_rate(ours, _unprompted(ours)):.1f}")
    L.append(f"- Wolf's hits: game {_hit_line(game)}")
    L.append(f"- Wolf's hits: ours {_hit_line(ours)}")
    for grp in sorted({k[0] for k in game["time"]} | {k[0] for k in ours["time"]}):
        gt = sum(v for k, v in game["time"].items() if k[0] == grp)
        ot = sum(v for k, v in ours["time"].items() if k[0] == grp)
        if gt < 5 and ot < 5:
            continue
        gu, ou = {id(a) for a in _unprompted(game)}, {id(a) for a in _unprompted(ours)}
        L += ["", f"anim group a{grp:03d}: game {gt:.0f} s, ours {ot:.0f} s (att/min: all / unprompted)",
              "Wolf at | game s | game att/min | ours s | ours att/min | game picks | ours picks", "--- | --- | --- | --- | --- | --- | ---"]
        for b, (lo, hi) in enumerate(BANDS):
            gs, os_ = game["time"].get((grp, b), 0), ours["time"].get((grp, b), 0)
            if gs < 2 and os_ < 2:
                continue
            ga = [n for n, _, d in game["attacks"] if _group(n) == grp and _band(d) == b]
            oa = [n for n, _, d in ours["attacks"] if _group(n) == grp and _band(d) == b]
            top = lambda xs: " ".join(f"{n[5:]}x{c}" for n, c in Counter(xs).most_common(4)) or "-"
            gua = [a for a in game["attacks"] if id(a) in gu and _group(a[0]) == grp and _band(a[2]) == b]
            oua = [a for a in ours["attacks"] if id(a) in ou and _group(a[0]) == grp and _band(a[2]) == b]
            rate = lambda n, u, s_: f"{60 * n / s_:.1f} / {60 * u / s_:.1f}" if s_ >= 2 else "-"
            L.append(f"{lo}-{hi if hi < 999 else ''} m | {gs:.0f} | {rate(len(ga), len(gua), gs)} | {os_:.0f} | {rate(len(oa), len(oua), os_)} | {top(ga)} | {top(oa)}")
    g, o = defaultdict(list), defaultdict(list)
    for n, _, d in game["attacks"]:
        g[n].append(d)
    for n, _, d in ours["attacks"]:
        o[n].append(d)
    gn, on = max(1, len(game["attacks"])), max(1, len(ours["attacks"]))
    L += ["", "attack | game n (share) | game median m | ours n (share) | ours median m", "--- | --- | --- | --- | ---"]
    for n in sorted(set(g) | set(o), key=lambda k: (-len(g.get(k, [])), -len(o.get(k, [])), k)):
        gd, od = g.get(n, []), o.get(n, [])
        L.append(f"{n} | {len(gd)} ({100 * len(gd) / gn:.0f}%) | {median(gd):.1f} | " if gd else f"{n} | 0 | - | ")
        L[-1] += f"{len(od)} ({100 * len(od) / on:.0f}%) | {median(od):.1f}" if od else "0 | -"
    only_game = [n for n in g if n not in o]
    only_ours = [n for n in o if n not in g]
    L += ["", f"- the game's attacks ours never started: {' '.join(sorted(only_game)) or 'none'}",
          f"- ours started, the game never did here: {' '.join(sorted(only_ours)) or 'none'}", ""]
    return L


def _merge(profs):
    out = {"secs": 0.0, "attacks": [], "hits": [], "time": Counter()}
    for p in profs:
        out["secs"] += p["secs"]
        out["time"].update(p["time"])
        out["attacks"] += p["attacks"]
        out["hits"] += p["hits"]
    return out


def enemies(rec, chr_=None, cached=False):
    """Per recording: [(chr, entity, game profile, our profile)]."""
    db = sqlite3.connect(ingest(rec))
    out = []
    for c, ent in engaged(db, chr_):
        prof = game_profile(db, ent)
        if prof["t0"] is None:
            continue
        tag = ent.split(":")[0]
        path = Path(rec) / f"enemy_ours_{tag}.csv"
        if not (cached and path.exists()):
            try:
                path = run_ours(rec, c, ent, prof, db)
            except SystemExit as e:
                print(f"{Path(rec).name} {c}: sim failed: {str(e)[:300]}")
                continue
        out.append((c, ent, prof, ours_profile(path)))
    db.close()
    return out


def report_one(rec, chr_=None, cached=False):
    L = [f"# {Path(rec).name}", ""]
    for c, ent, g, o in enemies(rec, chr_, cached):
        L += compare(c, g, o, ent)
    return L


def report_all(logs, cached=False):
    per = defaultdict(lambda: ([], [], []))
    for rec in sorted(Path(logs).glob("rec_*")):
        if not (rec / "rec.db").exists() and not (rec / "frames.bin").exists():
            continue
        for c, ent, g, o in enemies(rec, None, cached):
            per[c][0].append(g)
            per[c][1].append(o)
            per[c][2].append(rec.name[4:])
    L = ["# Enemies against the game's recordings", "",
         "Per kind over every recording it fought in. Ours: the same fight replayed (Wolf's recorded inputs,",
         "our AI live, neither dies), so the counts are a profile to compare, not a tick-by-tick match.", ""]
    for c in sorted(per, key=lambda k: -sum(len(p["attacks"]) for p in per[k][0])):
        g, o, recs = per[c]
        L += compare(c, _merge(g), _merge(o), f"{len(recs)} recording(s): {', '.join(recs[:4])}{' ...' if len(recs) > 4 else ''}")
    return L


# --- Pressure: how each kind takes Wolf's sword, closed loop ---------------------------------------
# The replay's Wolf swings where the game's enemy stood, so few of his attacks reach ours. Here our
# Wolf (locked on, walking in) presses attack every PRESS_EVERY ticks for PRESSURE_SECS against the
# kind's AI; the game's side is every recorded hit of Wolf's sword on that kind (his deflects out).
PRESS_EVERY = 40
PRESSURE_SECS = 120
REACT = {"deflect": "deflect", "guard": "guard", "damage": "damage", "attack": "kept on", "hyper armour": "kept on"}


def _react(name):
    return REACT.get(_anim_class(name), "other")


def game_reactions(logs):
    """{chr: (Counter of reactions, Counter of NpcParam rows)} over every recording."""
    out = defaultdict(lambda: (Counter(), Counter()))
    for rec in sorted(Path(logs).glob("rec_*")):
        if not (rec / "rec.db").exists():
            continue
        db = sqlite3.connect(rec / "rec.db")
        try:
            rows = db.execute("SELECT victim, reaction, attacker_anim FROM hits WHERE attacker_slot = 0").fetchall()
        except sqlite3.Error:
            continue
        for victim, reaction, aanim in rows:
            c, ent = _entity(victim)
            # Wolf's guard / deflect clips (a050_2xxxxx) hit it too: posture from his deflect
            if not ent or not aanim or aanim[5:6] == "2":
                continue
            out[c][0][_react(reaction)] += 1
            out[c][1][ent.split(":")[0][1:]] += 1
        db.close()
    return out


def pressure_inputs(path, secs=PRESSURE_SECS):
    head = "tick,stick_x,stick_y,attack,guard,step,jump,crouch,item,prosthetic,art,lock,anim,x,z,yaw,events,art_id,tool_id,lock_dx,lock_dz,hit_stop"
    rows = [head]
    for t in range(int(secs * 60)):
        press = int(t % PRESS_EVERY < 2 and t > 60)
        rows.append(f"{t},0,0.5,{press},0,0,0,0,0,0,0,1,,0,0,0,,,,,,")
    Path(path).write_text("\n".join(rows) + "\n", encoding="utf-8")


WOLF_DEFENCE = ("Guard", "Parry", "Deflect", "JustGuard")


def ours_reactions(path):
    rows = list(csv.DictReader(open(path, encoding="utf-8")))
    out, last, was = Counter(), -99, False
    for i, r in enumerate(rows):
        lost, gain = float(r.get("e_hp_lost") or 0), float(r.get("e_posture_gain") or 0)
        t = int(r["tick"])
        # a deathblow's clip (ThrowDef*) takes the HP the sim tops up every tick: not hits
        on = (lost > 0 or gain > 0) and not (r.get("e_state") or "").startswith("ThrowDef")
        new, was = on and not was, on
        # Wolf's sword, not his own guard / deflect (the enemy's swing bouncing off him); his
        # sword repelled by its guard reads EasyDeflected / HardDeflected and counts
        if new and t - last > 3 and not r["state"].startswith(WOLF_DEFENCE):
            out[_react(rows[min(i + REACT_AFTER, len(rows) - 1)].get("e_anim"))] += 1
            last = t
    return out


def pressure(logs, chr_=None):
    game = game_reactions(logs)
    d = Path(logs) / "pressure"
    d.mkdir(exist_ok=True)
    inputs = d / "inputs.csv"
    pressure_inputs(inputs)
    L = ["# Enemies under Wolf's sword (closed loop) against the game's", "",
         f"Ours: Wolf locked on, walking in, attack every {PRESS_EVERY} ticks for {PRESSURE_SECS} s (tools/rec/enemies.py",
         "pressure). Game: every recorded hit of Wolf's sword on the kind (his deflects left out). Shares of",
         "what the enemy did 8 ticks after: deflect (JustGuardDamage), guard, damage, kept on (its attack, hyper armour).", "",
         "kind | row | game hits | game deflect / guard / damage / kept on | ours hits | ours deflect / guard / damage / kept on",
         "--- | --- | --- | --- | --- | ---"]
    for c in sorted(game, key=lambda k: -sum(game[k][0].values())):
        if chr_ and c != chr_:
            continue
        g, rows = game[c]
        if sum(g.values()) < 10:
            continue
        row = rows.most_common(1)[0][0]
        out = d / f"ours_{c}_{row}.csv"
        try:
            run_sim(inputs, out, {"SHINOBI_REPLAY_ENEMY": c, "SHINOBI_REPLAY_ENEMY_ROW": row, "SHINOBI_REPLAY_ENEMY_AT": "0,-3"})
        except SystemExit as e:
            L.append(f"{c} | {row} | sim failed: {str(e)[:80]}")
            continue
        o = ours_reactions(out)
        share = lambda k: " / ".join(f"{100 * k[x] / max(1, sum(k.values())):.0f}" for x in ("deflect", "guard", "damage", "kept on"))
        L.append(f"{c} | {row} | {sum(g.values())} | {share(g)} | {sum(o.values())} | {share(o)}")
        print(L[-1])
    return L
