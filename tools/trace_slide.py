"""Foot sliding in a SHINOBI_TRACE csv (src/trace.rs): each foot's world position (Wolf's position +
the bone in his frame, rotated by his yaw), its horizontal speed per frame, and per script step the
median over 0.15 s windows of the slower foot's lowest speed - the planted foot. A clean walk/run
plants a foot at ~0 m/s; sliding shows as the planted foot moving with the body.
  python tools/trace_slide.py trace.csv"""
import csv, math, statistics, sys
from collections import defaultdict


def main():
    rows = list(csv.DictReader(open(sys.argv[1])))
    def world(r, b):
        x, z, yaw = float(r['x']), float(r['z']), float(r['yaw'])
        lx, lz = float(r[f'{b}_x']), float(r[f'{b}_z'])
        c, s = math.cos(yaw), math.sin(yaw)
        # Bevy rotation about Y by yaw: x' = c x + s z, z' = -s x + c z.
        return x + c * lx + s * lz, z - s * lx + c * lz
    speeds = defaultdict(list)
    for i in range(1, len(rows)):
        r, p = rows[i], rows[i - 1]
        dt = max(float(r['dt']), 1e-4)
        if r['step'] != p['step']:
            continue
        v = []
        for b in ('L_Foot', 'R_Foot'):
            (x1, z1), (x0, z0) = world(r, b), world(p, b)
            v.append(math.hypot(x1 - x0, z1 - z0) / dt)
        body = math.hypot(float(r['x']) - float(p['x']), float(r['z']) - float(p['z'])) / dt
        speeds[r['step']].append((float(r['clock']), min(v), body))
    print(f"{'step':24} {'body m/s':>9} {'planted foot m/s':>17}")
    for step, vals in speeds.items():
        t0 = vals[0][0]
        windows = defaultdict(list)
        for c, v, _ in vals:
            if c - t0 > 0.5:  # skip the start / blend
                windows[int((c - t0) / 0.15)].append(v)
        mins = [min(w) for w in windows.values() if w]
        body = statistics.median(b for _, _, b in vals)
        if mins and body > 0.3:
            print(f"{step:24} {body:9.2f} {statistics.median(mins):17.2f}")


if __name__ == '__main__':
    main()
