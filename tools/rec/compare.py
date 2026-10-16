"""Sekiro recording vs an Sv1 trace (SHINOBI_TRACE csv: clock,dt,step,state,anim,t,shown,blend,x,z,yaw,...).

Per anim: count, mean duration and mean horizontal speed in both, and the difference.
"""
import csv
import math
import sqlite3
from collections import defaultdict

from .fields import anim_name
from .ingest import ingest


def _sv1_segments(path):
    segs = defaultdict(list)  # anim -> [(dur s, dist m)]
    cur = None
    with open(path, newline="") as fh:
        for r in csv.DictReader(fh):
            a = r.get("anim", "")
            if a.isdigit():
                a = anim_name(int(a))
            try:
                t, x, z = float(r["clock"]), float(r["x"]), float(r["z"])
            except (KeyError, ValueError):
                continue
            if cur is None or a != cur[0]:
                if cur is not None:
                    segs[cur[0]].append((cur[3] - cur[1], math.hypot(cur[4] - cur[2][0], cur[5] - cur[2][1])))
                cur = [a, t, (x, z), t, x, z]
            else:
                cur[3], cur[4], cur[5] = t, x, z
    if cur is not None:
        segs[cur[0]].append((cur[3] - cur[1], math.hypot(cur[4] - cur[2][0], cur[5] - cur[2][1])))
    return segs


def compare(rec, sim, limit=40):
    db = sqlite3.connect(ingest(rec))
    sek = {r[0]: r[1:] for r in db.execute(
        "SELECT name, count(*), avg(dur_ms), sum(dist) / (sum(dur_ms) / 1000) FROM anims WHERE slot = 0 AND dur_ms > 0 GROUP BY name")}
    sv1 = _sv1_segments(sim)
    out = ["anim\tsekiro n\tms\tm/s\tsv1 n\tms\tm/s\tms diff %\tm/s diff %"]
    names = sorted(set(sek) & set(sv1), key=lambda n: -sek[n][0])
    for n in names[:limit]:
        sn, sms, ssp = sek[n]
        segs = sv1[n]
        vn = len(segs)
        vms = 1000 * sum(d for d, _ in segs) / vn
        tot = sum(d for d, _ in segs)
        vsp = sum(m for _, m in segs) / tot if tot > 0 else 0
        dms = 100 * (vms - sms) / sms if sms else 0
        dsp = 100 * (vsp - ssp) / ssp if ssp else 0
        out.append(f"{n}\t{sn}\t{sms:.0f}\t{(ssp or 0):.2f}\t{vn}\t{vms:.0f}\t{vsp:.2f}\t{dms:+.0f}\t{dsp:+.0f}")
    only = sorted(set(sek) - set(sv1), key=lambda n: -sek[n][0])[:15]
    out.append("in Sekiro only: " + " ".join(only))
    return out
