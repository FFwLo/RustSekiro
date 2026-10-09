"""HP / posture changes of a recorded character with the anims at that moment.
Data module block (+0x100): HP +0x30, posture (remaining) +0x48, max +0x4C.
  python tools/rec_hits.py <rec_dir> [slot]"""
import struct, sys
sys.path.insert(0, 'tools')
import rec_read as rr

def main():
    rec, slot = sys.argv[1], int(sys.argv[2]) if len(sys.argv) > 2 else 0
    names, by, recs = rr.load(rec)
    dm, ta = recs[(slot, by['data'])], recs[(slot, by['timeact'])]
    other = 1 if slot == 0 else 0
    ota = recs[(other, by['timeact'])]
    ticks = sorted(t for t in dm if t in ta)
    k = lambda a: f"a{a // 1000000:03d}_{a % 1000000:06d}"
    prev = None
    regen = []
    for t in ticks:
        raw = dm[t][1]
        hp, post, pmax = struct.unpack_from('<i', raw, 0x30)[0], struct.unpack_from('<i', raw, 0x48)[0], struct.unpack_from('<i', raw, 0x4C)[0]
        a = struct.unpack_from('<i', ta[t][1], 0x100)[0]
        oa = struct.unpack_from('<i', ota[t][1], 0x100)[0] if t in ota else 0
        if prev:
            dhp, dpost = hp - prev[0], post - prev[1]
            if dhp < 0 or dpost < -1:
                print(f"{dm[t][0]/1000:8.3f}s  hp {dhp:+5d} ({hp})  posture {dpost:+5d} ({post}/{pmax})  me {k(a)}  them {k(oa)}")
            elif dpost > 0:
                regen.append((dm[t][0] - dm[prev[2]][0], dpost, k(a)))
        prev = (hp, post, t)
    # regen per anim: posture points per second
    from collections import defaultdict
    agg = defaultdict(lambda: [0.0, 0])
    for dt, dp, a in regen:
        agg[a][0] += dt / 1000; agg[a][1] += dp
    print("posture regen (points/s while rising, by anim):")
    for a, (s, p) in sorted(agg.items(), key=lambda kv: -kv[1][0])[:10]:
        print(f"  {a} {p / s if s else 0:6.1f}/s over {s:.1f}s")

main()
