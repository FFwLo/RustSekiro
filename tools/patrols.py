"""Enemy patrol routes from the MSBs: extracted/enemies/patrols.json.

{map: {entity id: {part, model, npc, pos, yaw, type, points, areas, arrivals}}}, read by src/enemy.rs
(patrol_of). MSBS (SoulsFormats MSBS EventParam.cs PatrolInfo, PartParam Enemy):
- an enemy part's type data +0x20 is its WalkRouteIndex (i16, -1 none): the n-th event of
  type 14 PatrolInfo (the names match: c1700_0027 -> walk_route_c1700_0027_8094);
- PatrolInfo type data: PatrolType i32 (UnkT00), 3 x i32 0, 32 x i16 walk region indices (-1
  unused), 5 x WREntry {region i16, 0 i16, arrival anim i32, arrival wait i32}.
Points are the regions' positions (map space), `regions` their region indices (a route may pass
one region twice); the arrival entries are matched by region, as the exe does (FUN_1405c1ac0 /
FUN_1405c1b90, 1405c.c:1056 / 1133). PatrolType (the exe reads its low byte, FUN_1405c2080) is
stepped by src/ai.rs Patrol (the exe's table at 0x143af9550). The boss rows (npc_names.json
"rows") are left out.
`wake`: the map event that holds the enemy's AI (common_func 20005110: 2004[1] Set Character AI
State off, then on once Wolf (10000) is inside the region of 3[2] IF In/Outside Area, the enemy
has one of the 4[5] SpEffects, or Wolf damaged it (3[23] / 4[1])): {region: {shape, pos, yaw,
size}, sp_effects}. `route_end`: the map event that plays an anim once at the route's end
(route_ends): {region, anim}. Region shapes (MSBS Region +0x10, data at +0x48; SoulsFormats
ShapeType): 1 circle (r), 2 sphere (r), 3 cylinder (r, h), 4 rectangle (w, d), 5 box (w, d, h).
Usage: python tools/patrols.py
"""
import glob
import json
import os
import struct
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import boss_scripts as bs  # noqa: E402
from boss_events import u16s  # noqa: E402


def tables(d):
    out, o = {}, 0x10
    while True:
        _, cnt, name_off = struct.unpack_from('<iiq', d, o)
        offs = struct.unpack_from('<%dq' % (cnt - 1), d, o + 16)
        nxt = struct.unpack_from('<q', d, o + 16 + 8 * (cnt - 1))[0]
        out[u16s(d, name_off)] = list(offs)
        if nxt == 0:
            return out
        o = nxt


def holds(mp):
    """{entity: (region entity id, [SpEffect ids])} of the map's common_func 20005110 instances."""
    f = os.path.join(bs.ROOT, 'event', mp + '.emevd')
    if not os.path.exists(f):
        return {}
    out = {}
    for ev in bs.instances(bs.emevd(f), COMMON[0], COMMON[1]):
        if ev['id'] != 20005110:
            continue
        ins = ev['ins']
        off = next((v for b, i, v in ins if (b, i) == (2004, 1) and v[1] == 0), None)
        if not off:
            continue
        ent = off[0]
        region = next((v[3] for b, i, v in ins if (b, i) == (3, 2) and v[2] == 10000 and v[1] == 1), None)
        sps = [v[2] for b, i, v in ins if (b, i) == (4, 5) and v[1] == ent]
        out[ent] = (region, sps)
    return out


def route_ends(mp):
    """{entity: (region entity id, anim id)} of the map's route-end anim events (m11_01 11115236 /
    11115237 / 11115238): wait for the enemy inside the region (3[2] on itself) or its AI state
    1 / 2 / 3 (4[9], alerted); alerted, the event ends; else 2003[18] Force Animation Playback
    once (its Should Loop byte is never read by the exe: EzState plays Event<anim> to its end)."""
    f = os.path.join(bs.ROOT, 'event', mp + '.emevd')
    if not os.path.exists(f):
        return {}
    out = {}
    for ev in bs.instances(bs.emevd(f), COMMON[0], COMMON[1]):
        ins = ev['ins']
        for b, i, v in ins:
            if (b, i) != (2003, 18):
                continue
            ent = v[0]
            region = next((r[3] for bb, ii, r in ins if (bb, ii) == (3, 2) and r[2] == ent and r[1] == 1), None)
            alerted = sorted(r[2] for bb, ii, r in ins if (bb, ii) == (4, 9) and r[1] == ent and r[0] < 0)
            if region and alerted == [1, 2, 3]:
                out[ent] = (region, v[1])
    return out


COMMON = []


def routes(path, bosses):
    d = open(path, 'rb').read()
    t = tables(d)
    held = holds(os.path.basename(path)[:-4])
    ends = route_ends(os.path.basename(path)[:-4])
    models = [u16s(d, o + struct.unpack_from('<q', d, o)[0]) for o in t['MODEL_PARAM_ST']]
    # Regions: position @0x14 (MSBS Region).
    regions = [list(struct.unpack_from('<3f', d, o + 0x14)) for o in t.get('POINT_PARAM_ST', ())]
    rotations = [struct.unpack_from('<3f', d, o + 0x20)[1] for o in t.get('POINT_PARAM_ST', ())]

    def shape_of(o):
        shape = struct.unpack_from('<I', d, o + 0x10)[0]
        so = struct.unpack_from('<q', d, o + 0x48)[0]
        # MSB shape (SoulsFormats MSB/Shape.cs ShapeType): 1 Circle [r], 2 Sphere [r], 3 Cylinder
        # [r, h], 4 Rectangle [w, d], 5 Box [w, d, h].
        n = {1: 1, 2: 1, 3: 2, 4: 2, 5: 3}.get(shape, 0)
        if not (so and n):
            return None
        return {'shape': shape, 'pos': [round(v, 3) for v in struct.unpack_from('<3f', d, o + 0x14)],
                'yaw': round(struct.unpack_from('<3f', d, o + 0x20)[1], 3),
                'size': [round(v, 3) for v in struct.unpack_from('<%df' % n, d, o + so)]}

    shapes = {}
    for o in t.get('POINT_PARAM_ST', ()):
        rid = struct.unpack_from('<i', d, o + struct.unpack_from('<q', d, o + 0x50)[0] + 4)[0]
        if rid > 0 and shape_of(o):
            shapes[rid] = shape_of(o)
    patrols = []
    for o in t.get('EVENT_PARAM_ST', ()):
        _, _, typ, _ = struct.unpack_from('<qiii', d, o)
        if typ != 14:
            continue
        to = struct.unpack_from('<q', d, o + 0x20)[0]
        td = o + to
        kind = struct.unpack_from('<i', d, td)[0]
        walk = [i for i in struct.unpack_from('<32h', d, td + 16) if i >= 0]
        entries = [struct.unpack_from('<hhii', d, td + 16 + 64 + 12 * k) for k in range(5)]
        patrols.append((kind, walk, [(r, anim, wait) for r, _, anim, wait in entries if r >= 0]))
    out = {}
    for o in t['PARTS_PARAM_ST']:
        no, typ, _, mi = struct.unpack_from('<qIii', d, o)
        if typ not in (2, 10):
            continue
        eo, tdo = struct.unpack_from('<qq', d, o + 0x60)
        eid = struct.unpack_from('<i', d, o + eo)[0]
        td = o + tdo
        # Part.Enemy +0x8 ThinkParamID, +0xC NPCParamID: its own NpcThinkParam row (the roster's
        # first placement of the NpcParam row may have another).
        think = struct.unpack_from('<i', d, td + 0x8)[0]
        npc = struct.unpack_from('<i', d, td + 0xC)[0]
        route = struct.unpack_from('<h', d, td + 0x20)[0]
        if eid <= 0 or route < 0 or route >= len(patrols) or str(npc) in bosses:
            continue
        kind, walk, entries = patrols[route]
        if not walk:
            continue
        out[str(eid)] = {
            'part': u16s(d, o + no),
            'model': models[mi] if mi >= 0 else '',
            'npc': npc,
            'think': think,
            'pos': [round(v, 3) for v in struct.unpack_from('<3f', d, o + 0x20)],
            'yaw': round(struct.unpack_from('<3f', d, o + 0x2C)[1], 3),
            'type': kind,
            'points': [[round(v, 3) for v in regions[i]] for i in walk],
            'regions': walk,
            # Each point's region shape (null: a point region): the patrol reaches a point inside
            # it (IsInsideMovePoint FUN_1405e55c0 -> FUN_1409825b0, src/ai.rs Patrol::areas).
            'areas': [shape_of(t['POINT_PARAM_ST'][i]) for i in walk],
            # The regions' Y rotation (deg): a type 5 route's end makes the last one the
            # enemy's home pose (src/ai.rs Patrol).
            'yaws': [round(rotations[i], 3) for i in walk],
            # The arrival entries by their point's place in `points` (-1: not on the route).
            'arrivals': [{'region': r, 'point': walk.index(r) if r in walk else -1, 'anim': anim, 'wait': wait} for r, anim, wait in entries],
        }
        if eid in held:
            region, sps = held[eid]
            out[str(eid)]['wake'] = {'region': shapes.get(region), 'sp_effects': sps}
        if eid in ends:
            region, anim = ends[eid]
            out[str(eid)]['route_end'] = {'region': shapes.get(region), 'anim': anim}
    return out


def main():
    names = os.path.join(bs.ROOT, 'enemies', 'npc_names.json')
    bosses = set(json.load(open(names, encoding='utf-8')).get('rows', {})) if os.path.exists(names) else set()
    COMMON[:] = [bs.emevd(os.path.join(bs.ROOT, 'event', 'common_func.emevd')), bs.emedf()]
    out = {}
    for f in sorted(glob.glob(os.path.join(bs.ROOT, 'map', 'mapstudio', 'm*.msb'))):
        r = routes(f, bosses)
        if r:
            out[os.path.basename(f)[:-4]] = r
            print(f'{os.path.basename(f)}: {len(r)} patrolling enemies')
    path = os.path.join(bs.ROOT, 'enemies', 'patrols.json')
    with open(path, 'w', encoding='utf-8', newline='\n') as f:
        json.dump(out, f, indent=1, ensure_ascii=False)
    print(f'{path}: {sum(len(v) for v in out.values())} routes')


if __name__ == '__main__':
    main()
