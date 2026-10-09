"""Animation pops in a SHINOBI_TRACE csv (src/trace.rs): per bone (in Wolf's frame), the speed between
rendered frames; a frame is a pop when a bone's speed is far above its local median (the surrounding
+-10 frames) and above an absolute floor. Prints each pop with the state / anim around it, and per
script step how many pops it had.
  python tools/trace_pops.py trace.csv [ratio=4] [floor_m_per_s=1.5]"""
import csv, statistics, sys
from collections import Counter


def main():
    path = sys.argv[1]
    ratio = float(sys.argv[2]) if len(sys.argv) > 2 else 4.0
    floor = float(sys.argv[3]) if len(sys.argv) > 3 else 1.5
    rows = list(csv.DictReader(open(path)))
    bones = sorted({k[:-2] for k in rows[0] if k.endswith('_x') and k not in ('x',)})
    speeds = {b: [0.0] for b in bones}
    for i in range(1, len(rows)):
        dt = max(float(rows[i]['dt']), 1e-4)
        for b in bones:
            d = sum((float(rows[i][f'{b}_{c}']) - float(rows[i - 1][f'{b}_{c}'])) ** 2 for c in 'xyz') ** 0.5
            speeds[b].append(d / dt)
    pops, per_step = [], Counter()
    for i in range(2, len(rows) - 1):
        if rows[i]['step'] == 'warmup':
            continue
        worst = None
        for b in bones:
            s = speeds[b]
            ctx = s[max(1, i - 10):i] + s[i + 1:i + 11]
            med = statistics.median(ctx) if ctx else 0.0
            if s[i] > floor and s[i] > ratio * max(med, 0.15):
                score = s[i] / max(med, 0.15)
                if not worst or score > worst[0]:
                    worst = (score, b, s[i], med)
        if worst:
            r, p = rows[i], rows[i - 1]
            change = '' if (r['state'], r['anim']) == (p['state'], p['anim']) else f"  <- from {p['state']} {p['anim']} t={p['t']}"
            pops.append(i)
            per_step[r['step']] += 1
            print(f"{float(r['clock']):7.3f} {r['step']:22} {r['state']:28} {r['anim']:12} t={float(r['t']):.3f} "
                  f"{worst[1]:7} {worst[2]:6.2f} m/s (median {worst[3]:.2f}){change}")
    print('\npops per step:', dict(per_step))
    print(f'{len(rows)} frames, {len(pops)} pops')


if __name__ == '__main__':
    main()
