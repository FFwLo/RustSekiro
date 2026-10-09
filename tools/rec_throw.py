"""Paired-throw geometry from a memscope recording (deathblow / mikiri alignment check).

  python tools/rec_throw.py <rec_dir> <start_tick> [frames]

Per tick: Wolf (slot 0) and the throw partner (slot 1) body anims, the partner's position in
Wolf's frame (dist, angle from Wolf's facing) and the yaw difference. Physics +0x80 position,
+0x74 yaw (radians).
"""
import math
import struct
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
from rec_read import body_anims, load

rec, start = sys.argv[1], int(sys.argv[2])
n = int(sys.argv[3]) if len(sys.argv) > 3 else 120
names, by, recs = load(rec)
ph = [recs[(s, by["physics"])] for s in (0, 1)]
ba = [body_anims(recs[(s, by["timeact"])]) for s in (0, 1)]
ta = [recs[(s, by["timeact"])] for s in (0, 1)]
t0 = None
for t in sorted(ph[0]):
    if t < start or t not in ph[1]:
        continue
    if t0 is None:
        t0 = t
    if t - t0 > n:
        break
    p = [struct.unpack_from("<3f", ph[s][t][1], 0x80) for s in (0, 1)]
    yaw = [struct.unpack_from("<f", ph[s][t][1], 0x74)[0] for s in (0, 1)]
    dx, dz = p[1][0] - p[0][0], p[1][2] - p[0][2]
    dist = math.hypot(dx, dz)
    ang = math.degrees(math.atan2(dx, dz) - yaw[0])
    ang = (ang + 180) % 360 - 180
    dyaw = (math.degrees(yaw[1] - yaw[0]) + 180) % 360 - 180
    a = [f"a{ba[s][t]//1000000:03d}_{ba[s][t]%1000000:06d}" for s in (0, 1)]
    print(f"{t - t0:4d} wolf {a[0]} enemy {a[1]} dist {dist:5.2f} at {ang:7.1f} deg  dyaw {dyaw:7.1f}  dy {p[1][1]-p[0][1]:+.2f}")
