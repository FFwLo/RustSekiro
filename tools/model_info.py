"""Summary of exported game models (extracted/model_*.bin, SHMD format from sekiro-extract model).

  python tools/model_info.py                 every model: nodes / meshes / dummies
  python tools/model_info.py c1020 --meshes  one model's meshes: material -> albedo, vertex count
  python tools/model_info.py c0000 --dummy 142
  python tools/model_info.py c1020 --meshes --bones   dominant skin bones per mesh
  python tools/model_info.py c1020 --node sheath      matching nodes: bind transform + parents
"""
import argparse
import struct
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
sys.stdout.reconfigure(encoding="utf-8")


def read(path: Path):
    d = path.read_bytes()
    u16 = lambda o: struct.unpack_from("<H", d, o)[0]
    u32 = lambda o: struct.unpack_from("<I", d, o)[0]

    def s(o):
        n = u16(o)
        return o + 2 + n, d[o + 2:o + 2 + n].decode("utf-8", "replace")

    version, o = u32(4), 8
    nodes, parents, locals_ = [], [], []
    n = u32(o); o += 4
    for _ in range(n):
        o, name = s(o); nodes.append(name); parents.append(struct.unpack_from("<h", d, o)[0])
        locals_.append(struct.unpack_from("<10f", d, o + 2)); o += 42
    meshes = []
    m = u32(o); o += 4
    for _ in range(m):
        o, mat = s(o); o, alb = s(o)
        if version >= 3:
            o, _normal = s(o)
        vc = u32(o); o += 4
        bones = {}
        for v in range(vc):
            b = struct.unpack_from("<4H", d, o + v * 56 + 32); w = struct.unpack_from("<4f", d, o + v * 56 + 40)
            k = max(range(4), key=lambda i: w[i]); bones[b[k]] = bones.get(b[k], 0) + 1
        o += vc * 56
        ic = u32(o); o += 4 + ic * 4
        meshes.append((mat, alb, vc, bones))
    dummies = []
    if version >= 2 and o + 4 <= len(d):
        k = u32(o); o += 4
        for _ in range(k):
            # i16 id, i16 parent, i16 attach, f32 pos[3]
            dummies.append((struct.unpack_from("<h", d, o)[0], struct.unpack_from("<h", d, o + 4)[0], struct.unpack_from("<3f", d, o + 6)))
            o += 18
    return nodes, meshes, dummies, parents, locals_


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("filter", nargs="?", default="")
    ap.add_argument("--meshes", action="store_true")
    ap.add_argument("--dummy", type=int)
    ap.add_argument("--bones", action="store_true", help="with --meshes: dominant bones per mesh")
    ap.add_argument("--node", help="nodes whose name contains this: local bind transform and parent chain")
    a = ap.parse_args()
    for p in sorted((ROOT / "extracted").glob("model_*.bin")):
        if a.filter not in p.name:
            continue
        nodes, meshes, dummies, parents, locals_ = read(p)
        print(f"{p.name}: {len(nodes)} nodes, {len(meshes)} meshes ({sum(1 for m in meshes if m[1] == '-')} skipped), {len(dummies)} dummies")
        if a.meshes:
            for mat, alb, vc, bones in meshes:
                print(f"   {mat[:40]:40} -> {alb or '(none)'} ({vc} v)")
                if a.bones:
                    top = sorted(bones.items(), key=lambda kv: -kv[1])[:4]
                    print("      bones: " + ", ".join(f"{nodes[b] if b < len(nodes) else b}[{b}]({c})" for b, c in top))
        if a.node:
            for i, n in enumerate(nodes):
                if a.node.lower() in n.lower():
                    chain, j = [], parents[i]
                    while 0 <= j < len(nodes) and len(chain) < 6:
                        chain.append(nodes[j]); j = parents[j]
                    t = locals_[i]
                    print(f"   node {i} {n}: t=({t[0]:.3f},{t[1]:.3f},{t[2]:.3f}) q=({t[3]:.3f},{t[4]:.3f},{t[5]:.3f},{t[6]:.3f}) <- {' <- '.join(chain) or 'root'}")
        if a.dummy is not None:
            for i, att, pos in dummies:
                if i == a.dummy:
                    print(f"   dmy {i}: attach node {att} ({nodes[att] if 0 <= att < len(nodes) else '-'}), pos {tuple(round(x, 3) for x in pos)}")


if __name__ == "__main__":
    main()
