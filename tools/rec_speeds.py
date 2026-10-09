"""Steady horizontal speed per anim from a recording (physics +0x80 position, TimeAct +0x100 anim).
Skips the first 0.3 s of each segment (acceleration); segments shorter than 0.6 s are ignored.
  python tools/rec_speeds.py <rec_dir> [slot]"""
import json, math, struct, sys
from collections import defaultdict
sys.path.insert(0, 'tools')
import rec_read as rr

def main():
    rec, slot = sys.argv[1], int(sys.argv[2]) if len(sys.argv) > 2 else 0
    names, by, recs = rr.load(rec)
    d = json.load(open('extracted/combat_data.json', encoding='utf-8'))
    P = d['player'] if slot == 0 else d['enemy']
    state_of = {}
    for s, k in P['states'].items():
        state_of.setdefault(k, s)
    ta, ph = recs[(slot, by['timeact'])], recs[(slot, by['physics'])]
    ticks = sorted(t for t in ta if t in ph)
    BA = rr.body_anims(ta)
    anim = {t: BA[t] for t in ticks}
    pos = {t: struct.unpack_from('<3f', ph[t][1], 0x80) for t in ticks}
    clock = {t: ta[t][0] / 1000 for t in ticks}
    speeds = defaultdict(list)
    start = ticks[0]
    for a, b in zip(ticks, ticks[1:] + [None]):
        if b is None or anim[b] != anim[a]:
            seg = [t for t in ticks if start <= t <= a and clock[t] - clock[start] >= 0.3]
            if len(seg) > 2 and clock[a] - clock[start] >= 0.6:
                p0, p1 = pos[seg[0]], pos[seg[-1]]
                dist = math.hypot(p1[0] - p0[0], p1[2] - p0[2])
                speeds[anim[a]].append(dist / (clock[seg[-1]] - clock[seg[0]]))
            start = b
    print("anim            state                         n   median m/s  (min-max)")
    for a, v in sorted(speeds.items(), key=lambda kv: -len(kv[1])):
        v.sort()
        k = f"a{a // 1000000:03d}_{a % 1000000:06d}"
        print(f"{k}  {state_of.get(k, '-'):28s} {len(v):3d}   {v[len(v)//2]:6.2f}    ({v[0]:.2f}-{v[-1]:.2f})")

main()
