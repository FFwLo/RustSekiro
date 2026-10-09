"""Buffered-input windows from a recording vs our TAE data.
For each anim X -> next action Y where Y's button was pressed while X was still playing (buffered),
the real frame Y began = the frame the game opened that input. Compared with the first frame our
data allows it (attack: combo refs 214-219/231 or cancel flag 115; guard: flag 117; step: 26/25).
  python tools/rec_windows.py <rec_dir>"""
import json, struct, sys
from collections import defaultdict
sys.path.insert(0, 'tools')
import rec_read as rr

ATTACK = {50300000, 50300001, 50300010, 50300020, 50300030, 50300040, 50300011}
GUARD = {50203000, 50203002, 50203005, 50203006, 50203007}
BTN = {"attack": (0xF4, ATTACK), "guard": (0xF5, GUARD)}

def key(a):
    return f"a{a // 1000000:03d}_{a % 1000000:06d}"

def main():
    names, by, recs = rr.load(sys.argv[1])
    d = json.load(open('extracted/combat_data.json', encoding='utf-8'))
    P = d['player']; se = P['spEffects']
    state_of = {}
    for s, k in P['states'].items():
        state_of.setdefault(k, s)
    ta, pad = recs[(0, by['timeact'])], recs[(0, by['pad'])]
    ticks = sorted(t for t in ta if t in pad)
    BA = rr.body_anims(ta)
    anim = {t: BA[t] for t in ticks}
    ms = lambda t: ta[t][0]

    def our_first(k, what):
        a = P['anims'].get(k)
        if not a:
            return None
        frames = []
        for e in a['events']:
            args = e.get('args', {})
            if what == "attack" and e['type'] == 0 and str(args.get('FlagType', '')).startswith('115'):
                frames.append(e['startFrame'])
            if what == "guard" and e['type'] == 0 and str(args.get('FlagType', '')).startswith('117'):
                frames.append(e['startFrame'])
        return min(frames) if frames else None

    # segments
    segs, cur, start = [], None, None
    for t in ticks:
        if anim[t] != cur:
            if cur is not None:
                segs.append((start, t, cur))
            cur, start = anim[t], t
    out = defaultdict(list)
    for (s0, e0, a0), (s1, e1, a1) in zip(segs, segs[1:]):
        for what, (byte, targets) in BTN.items():
            if a1 not in targets or a0 in (a1,):
                continue
            # rising edge of the button inside X, before the switch (buffered)
            seg_ticks = [t for t in ticks if s0 <= t < s1]
            pressed = [t for p, t in zip(seg_ticks, seg_ticks[1:]) if (pad[t][1][byte] & 1) and not (pad[p][1][byte] & 1)]
            if not pressed or (ms(s1) - ms(pressed[0])) < 1000 / 30:
                continue
            out[(key(a0), what)].append(((ms(s1) - ms(s0)) / 1000 * 30, (ms(pressed[0]) - ms(s0)) / 1000 * 30, [round((ms(p) - ms(s0)) / 1000 * 30, 1) for p in pressed], [round((ms(t) - ms(s0)) / 1000 * 30, 1) for p, t in zip(seg_ticks, seg_ticks[1:]) if not (pad[t][1][byte] & 1) and (pad[p][1][byte] & 1)]))
    print("from anim / state / next / n / real first, median switch frame / our flag start / samples press>switch (TAE frames)")
    for (k, what), v in sorted(out.items()):
        v.sort()
        samples = " ".join(f"{p:.1f}>{t:.1f} presses{ps} letgo{lg}" for t, p, ps, lg in v[:4])
        print(f"{k:14s} {state_of.get(k, '-'):32s} {what:7s} {len(v):2d}  {v[0][0]:5.1f} / {v[len(v)//2][0]:5.1f}  ours {our_first(k, what)}  press>switch {samples}")

main()
