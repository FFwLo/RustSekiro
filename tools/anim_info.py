"""Compact look-ups in extracted/combat_data.json (replaces ad-hoc snippets).

  python tools/anim_info.py <state|anim key> [...]     events of one or more states (frames at 30 fps)
  python tools/anim_info.py --ref 231                   every anim raising SpEffect behaviour ref 231
  python tools/anim_info.py --event 153                 every anim with TAE event type 153 (+ args)
  python tools/anim_info.py --state Sprint              states whose name contains "Sprint" -> anim key
Options: --enemy (enemy data instead of Wolf), --types 0,67,1 (filter event types in the state view).
State view: refs (SpEffect behaviorRefId), ChrActionFlags, then other events grouped by type.
"""
import argparse
import json
import sys
from collections import defaultdict
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
sys.stdout.reconfigure(encoding="utf-8")


def load(enemy: bool):
    d = json.loads((ROOT / "extracted" / "combat_data.json").read_text(encoding="utf-8"))
    c = d["enemy"] if enemy else d["player"]
    inv = defaultdict(list)
    for s, k in c["states"].items():
        inv[k].append(s)
    return c, inv


def fr(e):
    return f"{round(e['startFrame'])}-{round(e['endFrame'])}"


def ref_of(c, e):
    sid = str(e["args"].get("SpEffectID", ""))
    return c["spEffects"][sid]["behaviorRefId"] if sid in c["spEffects"] else None


def short_args(e, keep=4):
    a = {k: v for k, v in e["args"].items() if k not in ("StateInfo",) and not k.startswith("unk") or v}
    items = list(a.items())[:keep]
    return ", ".join(f"{k}={v}" for k, v in items)


def show_state(c, inv, name, types):
    key = c["states"].get(name, name if name in c["anims"] else None)
    if key is None:
        print(f"{name}: not found (try --state {name})")
        return
    an = c["anims"][key]
    dur = an.get("duration")
    print(f"{name} -> {key}  {round(dur * 30) if dur else '?'} f  (states: {', '.join(inv.get(key, [])) or '-'})")
    refs, flags, other = [], [], defaultdict(list)
    for e in an["events"]:
        if types and e["type"] not in types:
            continue
        r = ref_of(c, e)
        gated = " [gated]" if e["args"].get("StateInfo") else ""
        if r is not None:
            refs.append(f"{r}@{fr(e)}{gated}")
        elif e["type"] == 0:
            flags.append(f"{str(e['args'].get('FlagType', '?')).split(':')[0]}@{fr(e)}")
        else:
            other[(e["type"], e["name"])].append(f"{fr(e)}{gated} {short_args(e)}".strip())
    if refs:
        print("  refs :", " ".join(refs))
    if flags:
        print("  flags:", " ".join(flags))
    for (t, n), v in sorted(other.items()):
        print(f"  {t} {n}: " + " | ".join(v[:6]) + (f" (+{len(v) - 6})" if len(v) > 6 else ""))


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("names", nargs="*")
    ap.add_argument("--enemy", action="store_true")
    ap.add_argument("--ref", type=int)
    ap.add_argument("--event", type=int)
    ap.add_argument("--state")
    ap.add_argument("--types", default="")
    a = ap.parse_args()
    c, inv = load(a.enemy)
    if a.ref is not None:
        for k, an in c["anims"].items():
            hits = [fr(e) for e in an["events"] if ref_of(c, e) == a.ref]
            if hits:
                print(f"{k} {inv.get(k, ['-'])[0]}: {' '.join(hits)}")
    if a.event is not None:
        for k, an in c["anims"].items():
            for e in an["events"]:
                if e["type"] == a.event:
                    print(f"{k} {inv.get(k, ['-'])[0]} {fr(e)}: {short_args(e, 8)}")
    if a.state:
        for s, k in sorted(c["states"].items()):
            if a.state.lower() in s.lower():
                print(f"{s} -> {k}")
    types = {int(t) for t in a.types.split(",") if t}
    for n in a.names:
        show_state(c, inv, n, types)


if __name__ == "__main__":
    main()
