"""Anim timeline of one recorded character: segments of TimeAct +0x100 (anim id = group * 1e6 + number).
  python tools/rec_timeline.py <rec_dir> [slot] > timeline.txt
Columns: start s, real duration s, anim key, state name (our export), TAE duration (our export)."""
import json, struct, sys
sys.path.insert(0, 'tools')
import rec_read as rr

def main():
    rec, slot = sys.argv[1], int(sys.argv[2]) if len(sys.argv) > 2 else 0
    names, by, recs = rr.load(rec)
    d = json.load(open('extracted/combat_data.json', encoding='utf-8'))
    P = d['player'] if slot == 0 else d['enemy']
    key_state = {}
    for s, k in P['states'].items():
        key_state.setdefault(k, s)
    ta = recs[(slot, by['timeact'])]
    ticks = sorted(ta)
    BA = rr.body_anims(ta)
    segs, cur, start = [], None, None
    for t in ticks:
        a = BA[t]
        if a != cur:
            if cur is not None:
                segs.append((start, t, cur))
            cur, start = a, t
    segs.append((start, ticks[-1], cur))
    for s, e, a in segs:
        k = f"a{a // 1000000:03d}_{a % 1000000:06d}"
        dur = P['anims'].get(k, {}).get('duration')
        real = (ta[e][0] - ta[s][0]) / 1000
        print(f"{ta[s][0]/1000:8.3f} {real:6.3f} {k} {key_state.get(k, '-'):34s} {round(dur, 3) if dur else '-'}")

main()
