"""The maps' authored grass -> extracted/map_grass.json, and the grass models' ids ->
extracted/map_grass_models.txt (exported by tools/extract.ps1 with `sekiro-extract objmesh`).

Each map piece with a grass config (MSBS PARTS_PARAM_ST MapPiece, SoulsFormatsNEXT
MSBS/PartsParam.cs: name @0x00, type @0x08 (0 MapPiece), model index @0x10, position @0x20,
rotation (degrees) @0x2C, scale @0x38, UnkStruct7 offset @0x80 = six GrassTypeParam ids, one per
painted density channel) has a `<map>_<model>.grass` table in its mapbnd: a painted triangle
surface, each vertex with six densities. GrassTypeParam (Paramdex SDT/Defs/GrassTypeParam.xml)
names the clump model (modelName) and how it is laid out.

After sekiro-rs (MIT / Apache-2.0, AKJama) crates/extract/src/maps/grass.rs: the GRASS layout
(header 1, 40, 20, volumes, 36, vertices, 24, faces, 24, volumes; 20-byte volumes, then
vertices of position + six densities, then faces of normal + three indices), the surface taken
into map space by the piece's transform before it is measured, and the layout:
- distributionType 0: area x mean painted density x baseDensity clumps a face (the fraction
  carried to the next face), at Halton (2, 3) points warped toward the denser corners
  (weight x (density + 1/3));
- distributionType 1: a hex grid of `spacing` over the face's XZ, a point kept when a random
  draw is under its painted density x baseDensity (always with dithering 3);
- each clump tilted toward the face's normal by normalInfluence %, up to inclinationMax, turned
  by orientationAngle (random when negative), scaled by scaleBase / scaleHeight Min..Max (%).
gap: the game's own random seed, its BVH leaf grouping and the hex dither pattern are not
traced (sekiro-rs's stable generator is used), nor the wind (windAmplitude / windCycle) and the
colour shades.

Output: {map: [{model, type, clumps: [x, y, z, qx, qy, qz, qw, base, height] x n}]} in MSB space;
the clump models' materials' albedo (their FLVERs leave the texture paths to the MTD,
`M_Translucency__e_snp_Texture2D_2_AlbedoMap`) -> extracted/map_grass_albedo.json {mtd: texture}.
Usage: python tools/map_grass.py (after the objmesh exports of map_grass_models.txt)
"""
import glob
import json
import math
import os
import re
import struct
import subprocess
import sys

sys.path.insert(0, os.path.dirname(__file__))
from map_fx import tables, u16s  # noqa: E402

ROOT = os.path.join(os.path.dirname(__file__), '..', 'extracted')
M64 = (1 << 64) - 1


def parse(data):
    h = struct.unpack_from('<10I', data, 0)
    if h[0:3] != (1, 40, 20) or h[4] != 36 or h[6] != 24 or h[8] != 24 or h[3] != h[9]:
        raise ValueError('invalid GRASS header')
    vols, nv, nf = h[3], h[5], h[7]
    if 40 + vols * 44 + nv * 36 + nf * 24 != len(data):
        raise ValueError('invalid GRASS table lengths')
    o = 40 + vols * 20
    verts = []
    for i in range(nv):
        v = struct.unpack_from('<9f', data, o + i * 36)
        verts.append((v[0:3], v[3:9]))
    o += nv * 36
    faces = []
    for i in range(nf):
        n = struct.unpack_from('<3f', data, o + i * 24)
        idx = struct.unpack_from('<3I', data, o + i * 24 + 12)
        faces.append((idx, n))
    return verts, faces


# Small vector / quaternion helpers (x, y, z) and (x, y, z, w).
def add(a, b): return (a[0] + b[0], a[1] + b[1], a[2] + b[2])
def sub(a, b): return (a[0] - b[0], a[1] - b[1], a[2] - b[2])
def mul(a, s): return (a[0] * s, a[1] * s, a[2] * s)
def dot(a, b): return a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
def cross(a, b): return (a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0])
def length(a): return math.sqrt(dot(a, a))


def norm(a, default=(0.0, 1.0, 0.0)):
    n = length(a)
    return mul(a, 1.0 / n) if n > 1e-12 else default


def qmul(a, b):
    ax, ay, az, aw = a
    bx, by, bz, bw = b
    return (aw * bx + ax * bw + ay * bz - az * by, aw * by - ax * bz + ay * bw + az * bx,
            aw * bz + ax * by - ay * bx + az * bw, aw * bw - ax * bx - ay * by - az * bz)


def qaxis(axis, ang):
    s = math.sin(ang / 2)
    return (axis[0] * s, axis[1] * s, axis[2] * s, math.cos(ang / 2))


def qrot(q, v):
    p = qmul(qmul(q, (v[0], v[1], v[2], 0.0)), (-q[0], -q[1], -q[2], q[3]))
    return p[:3]


def qarc(a, b):
    """glam Quat::from_rotation_arc(a, b) for unit vectors."""
    d = dot(a, b)
    if d < -0.999999:
        return (1.0, 0.0, 0.0, 0.0)
    c = cross(a, b)
    q = (c[0], c[1], c[2], 1.0 + d)
    n = math.sqrt(sum(x * x for x in q))
    return tuple(x / n for x in q)


def qslerp(a, b, t):
    d = sum(x * y for x, y in zip(a, b))
    if d < 0:
        b, d = tuple(-x for x in b), -d
    if d > 0.9995:
        q = tuple(x + (y - x) * t for x, y in zip(a, b))
    else:
        th = math.acos(d)
        s = math.sin(th)
        q = tuple((math.sin((1 - t) * th) * x + math.sin(t * th) * y) / s for x, y in zip(a, b))
    n = math.sqrt(sum(x * x for x in q))
    return tuple(x / n for x in q)


def part_rotation(rot):
    """MSB rotation (degrees): Ry * Rz * Rx (sekiro-extract msb.rs `rotate`)."""
    r = [math.radians(v) for v in rot]
    return qmul(qmul(qaxis((0, 1, 0), r[1]), qaxis((0, 0, 1), r[2])), qaxis((1, 0, 0), r[0]))


class Random:
    """sekiro-rs's stable generator (splitmix64 step); the game's seed is not traced."""
    def __init__(self, seed):
        self.s = seed & M64

    def next(self):
        self.s = (self.s + 0x9e3779b97f4a7c15) & M64
        z = self.s
        z = ((z ^ (z >> 30)) * 0xbf58476d1ce4e5b9) & M64
        z = ((z ^ (z >> 27)) * 0x94d049bb133111eb) & M64
        return ((z ^ (z >> 31)) >> 40) / 16777216.0


def radical_inverse(i, base):
    r, f = 0.0, 1.0
    while i > 0:
        f /= base
        r += (i % base) * f
        i //= base
    return r


def sample(verts, faces, ch, row, seed):
    density, spacing = row['baseDensity'], row['spacing']
    rnd = Random(seed)
    out, remainder, seq = [], 0.0, seed % 33
    for idx, normal in faces:
        a, b, c = (verts[i] for i in idx)
        pa, pb, pc = a[0], b[0], c[0]
        area = length(cross(sub(pb, pa), sub(pc, pa))) * 0.5
        painted = (a[1][ch], b[1][ch], c[1][ch])
        expected = area * density * sum(painted) / 3.0
        if expected == 0:
            continue
        pts = []
        if row['distributionType'] == 0:
            expected += remainder
            count = int(math.floor(expected))
            remainder = expected - count
            for _ in range(count):
                u = math.sqrt(radical_inverse(seq, 2))
                v = radical_inverse(seq, 3)
                seq += 1
                w = [1 - u, u * (1 - v), u * v]
                w = [x * (p + 1 / 3) for x, p in zip(w, painted)]
                s = sum(w)
                pts.append([x / s for x in w])
        elif row['distributionType'] == 1:
            lo = [min(pa[i], pb[i], pc[i]) for i in range(3)]
            hi = [max(pa[i], pb[i], pc[i]) for i in range(3)]
            dz = spacing * math.sqrt(3) * 0.5
            ab = (pb[0] - pa[0], pb[2] - pa[2])
            ac = (pc[0] - pa[0], pc[2] - pa[2])
            det = ab[0] * ac[1] - ab[1] * ac[0]
            if abs(det) < 1e-8:
                continue
            for iz in range(math.floor(lo[2] / dz), math.ceil(hi[2] / dz) + 1):
                off = spacing * 0.5 if iz & 1 == 0 else 0.0
                for ix in range(math.floor(lo[0] / spacing) - 1, math.ceil(hi[0] / spacing) + 1):
                    d = (ix * spacing + off - pa[0], iz * dz - pa[2])
                    v = (d[0] * ac[1] - d[1] * ac[0]) / det
                    w = (ab[0] * d[1] - ab[1] * d[0]) / det
                    ws = [1 - v - w, v, w]
                    if any(x < 0 for x in ws):
                        continue
                    cover = sum(x * p for x, p in zip(ws, painted)) * density
                    if row['dithering'] == 3 or rnd.next() < cover:
                        pts.append(ws)
        else:
            continue
        n = norm(normal)
        ang = math.acos(max(-1.0, min(1.0, n[1])))
        max_ang = math.radians(row['inclinationMax'])
        influence = row['normalInfluence'] * 0.01
        align = qslerp((0.0, 0.0, 0.0, 1.0), qarc((0.0, 1.0, 0.0), n), influence * min(max_ang / ang, 1.0) if ang > 0 else 0.0)
        for w in pts:
            p = add(add(mul(pa, w[0]), mul(pb, w[1])), mul(pc, w[2]))
            yaw = rnd.next() * math.tau if row['orientationAngle'] < 0 else math.radians(row['orientationAngle'])
            q = qmul(align, qaxis((0, 1, 0), yaw))
            base = (row['scaleBaseMin'] + rnd.next() * (row['scaleBaseMax'] - row['scaleBaseMin'])) * 0.01
            height = (row['scaleHeightMin'] + rnd.next() * (row['scaleHeightMax'] - row['scaleHeightMin'])) * 0.01
            out.extend(round(x, 3) for x in (*p, *q, base, height))
    return out


def grass_parts(path):
    """[(model name, position, rotation, scale, six GrassTypeParam ids)] of an MSB's map pieces."""
    d = open(path, 'rb').read()
    t = tables(d)
    models = [u16s(d, o + struct.unpack_from('<q', d, o)[0]) for o in t.get('MODEL_PARAM_ST', ())]
    out = []
    for o in t.get('PARTS_PARAM_ST', ()):
        if struct.unpack_from('<I', d, o + 8)[0] != 0:
            continue
        u7 = struct.unpack_from('<q', d, o + 0x80)[0]
        if not u7:
            continue
        ids = struct.unpack_from('<6i', d, o + u7)
        if any(i > 0 for i in ids):
            m = models[struct.unpack_from('<i', d, o + 0x10)[0]]
            pos, rot, scale = (struct.unpack_from('<3f', d, o + k) for k in (0x20, 0x2C, 0x38))
            out.append((m, pos, rot, scale, ids))
    return out


def main():
    rows = {r['id']: r for r in json.load(open(os.path.join(ROOT, 'json', 'params', 'GrassTypeParam.json'), encoding='utf-8'))['rows']}
    result, models = {}, set()
    for path in sorted(glob.glob(os.path.join(ROOT, 'map', 'mapstudio', 'm*_*_00_00.msb'))):
        m = os.path.basename(path)[:-4]
        groups, surfaces, total = {}, {}, 0
        for model, pos, rot, scale, ids in grass_parts(path):
            g = os.path.join(ROOT, 'map', m, '%s_%s.mapbnd.d' % (m, model[1:]), '%s_%s.grass' % (m, model[1:]))
            if not os.path.exists(g):
                continue
            if g not in surfaces:
                surfaces[g] = parse(open(g, 'rb').read())
            verts, faces = surfaces[g]
            # Into map space: scale, the part's turn, its position; normals by the inverse
            # transpose (the turn of 1 / scale).
            q = part_rotation(rot)
            wv = [(add(qrot(q, (p[0] * scale[0], p[1] * scale[1], p[2] * scale[2])), pos), dens) for p, dens in verts]
            wf = [(idx, norm(qrot(q, (n[0] / scale[0], n[1] / scale[1], n[2] / scale[2])))) for idx, n in faces]
            for ch, tid in enumerate(ids):
                if tid <= 0 or tid not in rows or not any(v[1][ch] > 0 for v in verts):
                    continue
                row = rows[tid]
                seed = 0
                for b in ('%s-%d-%d' % (model, ch, tid)).encode():
                    seed = (seed * 131 + b) & M64
                clumps = sample(wv, wf, ch, row, seed)
                if not clumps:
                    continue
                name = row['modelName']
                models.add(name)
                groups.setdefault((name, tid), []).extend(clumps)
                total += len(clumps) // 9
        if groups:
            result[m] = [{'model': k[0], 'type': k[1], 'clumps': v} for k, v in sorted(groups.items())]
            print('%s: %d clumps of %d models' % (m, total, len({k[0] for k in groups})))
    json.dump(result, open(os.path.join(ROOT, 'map_grass.json'), 'w'), separators=(',', ':'))
    open(os.path.join(ROOT, 'map_grass_models.txt'), 'w').write('\n'.join(sorted(models)) + '\n')
    print('%d grass models' % len(models))
    albedos(models)


def albedos(models):
    exe = os.path.join(os.path.dirname(__file__), 'sekiro-extract', 'target', 'release', 'sekiro-extract.exe')
    mtd_dir = os.path.join(ROOT, 'mtd', 'allmaterialbnd.mtdbnd.d')
    files = {f.lower(): f for f in os.listdir(mtd_dir)}
    out = {}
    for m in sorted(models):
        path = os.path.join(ROOT, 'objmesh', m + '.json')
        if not os.path.exists(path):
            continue
        for mesh in json.load(open(path))['meshes']:
            name = re.split(r'[\\/]', mesh['mtd'])[-1]
            key = name.lower() if name.lower().endswith('.mtd') else name.lower() + '.mtd'
            if name in out or key not in files:
                continue
            text = subprocess.run([exe, 'mtd', os.path.join(mtd_dir, files[key])], capture_output=True, text=True).stdout
            hit = re.search(r'slot \S*AlbedoMap\S* uv \d+ path "([^"]*)"', text)
            if hit:
                # The file name before its extension (o007590's path carries a stray CR LF).
                tex = re.search(r'([^\\/]+?)\.(?:tif|tga|dds|psd|png)', hit.group(1), re.I)
                if tex:
                    out[name] = tex.group(1).lower()
    json.dump(out, open(os.path.join(ROOT, 'map_grass_albedo.json'), 'w'), indent=1)
    print('%d grass materials' % len(out))


if __name__ == '__main__':
    main()
