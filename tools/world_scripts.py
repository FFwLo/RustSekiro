"""World map scripts -> extracted/enemies/world_scripts.json, run by src/enemy/world.rs.

Every event instance a base map (mXX_YY_00_00) initialises (event 50 / 0 and the common_func
events they start, tools/boss_scripts.py `instances`), with the map's enemy parts and the regions
and places its events name, for the open world (sekiro-rs's map): the same layout as a boss
script (src/enemy/script.rs ScriptDef) with "world": true, and positions raw in MSB space (no
boss frame: src/enemy/script.rs Run::place_of turns them into the world's Bevy space, X
mirrored).

Output, keyed by map: {"map", "boss": 0, "world": true,
  "chars": {entity: {"chr", "npc", "think", "at": [x, y, z], "yaw", "gate"?: [flag, on]}} (every
           enemy part: the NPCs, the named enemies and the bosses too, at their own places,
           enabled or not by their map's events; "gate" leaves the part out while that flag is
           `on`, e.g. the Demon of Hatred until the burnt Outskirts' 8302; not the c1000 / c1001
           event dummies),
  "places": {entity: {"at", "yaw"}} (regions, points and the object parts the events name),
  "regions": {entity: {"shape", "size", "parts"}},
  "events": [{"id", "slot", "rest", "ins"}], "flags_on": [],
  "objacts": [{"id", "object", "param", "flag", "dist", "player_angle", "obj_angle", "anim",
               "dummy", "at", "forward"}] (the MSB's ObjAct events, `objacts`; their objects are
           in "places")}
Usage: python tools/world_scripts.py
"""
import json
import math
import os
import struct
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import boss_events as be  # noqa: E402
import boss_scripts as bs  # noqa: E402

# Event helpers with no body of their own (the boss bars' and bullets' stand-ins).
DUMMIES = {'c1000', 'c1001'}


def typedata(path):
    """{entity: (NpcThinkParam row, EventFlagID, EventFlagCompareState, entity groups)} of the
    enemy parts (Part.Enemy typedata +0x8 ThinkParamID, +0x40 / +0x44: the part is left out
    while that flag equals the state; entity data +0x1C, 8 EntityGroupIDs)."""
    d = open(path, 'rb').read()
    tables, o = {}, 0x10
    while True:
        _, cnt, name_off = struct.unpack_from('<iiq', d, o)
        offs = struct.unpack_from('<%dq' % (cnt - 1), d, o + 16)
        nxt = struct.unpack_from('<q', d, o + 16 + 8 * (cnt - 1))[0]
        tables[be.u16s(d, name_off)] = offs
        if nxt == 0:
            break
        o = nxt
    out = {}
    for o in tables['PARTS_PARAM_ST']:
        typ = struct.unpack_from('<I', d, o + 8)[0]
        if typ not in (2, 10):
            continue
        eo, tdo = struct.unpack_from('<qq', d, o + 0x60)
        eid = struct.unpack_from('<i', d, o + eo)[0]
        if eid > 0:
            groups = [g for g in struct.unpack_from('<8i', d, o + eo + 0x1C) if g > 0]
            out[eid] = (struct.unpack_from('<i', d, o + tdo + 0x8)[0],) + struct.unpack_from('<ii', d, o + tdo + 0x40) + (groups,)
    return out


def group_gates(td):
    """{entity: (flag, state)} for the parts with no flag of their own whose entity groups'
    flagged members all share one (EventFlagID, CompareState): the Outskirts' Demon of Hatred
    group 1105610 (his eight c7021 helpers, two burnt-state soldiers) and soldier group 1105600,
    which m11_00_70_10's event 50 hides by the same flags (common_func 20004111)."""
    of_group = {}
    for flag, state, groups in ((f, s, g) for _, f, s, g in td.values()):
        for g in groups:
            of_group.setdefault(g, set()).add((flag, state) if flag > 0 else None)
    out = {}
    for eid, (_, flag, _, groups) in td.items():
        shared = set().union(*(of_group[g] - {None} for g in groups)) if groups else set()
        if flag <= 0 and len(shared) == 1:
            out[eid] = shared.pop()
    return out


def object_places(path):
    """{entity id: ((x, y, z), yaw degrees)} of the object parts (MSBS Part types 1 Object and
    9 DummyObject; position @0x20, rotation @0x2C, entity data @0x60): the doors, gates and
    breakables the map's events name (an In/Outside Radius check on a breakable, m11_01
    20005546)."""
    d = open(path, 'rb').read()
    tables, o = {}, 0x10
    while True:
        _, cnt, name_off = struct.unpack_from('<iiq', d, o)
        offs = struct.unpack_from('<%dq' % (cnt - 1), d, o + 16)
        nxt = struct.unpack_from('<q', d, o + 16 + 8 * (cnt - 1))[0]
        tables[be.u16s(d, name_off)] = offs
        if nxt == 0:
            break
        o = nxt
    out = {}
    for o in tables['PARTS_PARAM_ST']:
        if struct.unpack_from('<I', d, o + 8)[0] not in (1, 9):
            continue
        eid = struct.unpack_from('<i', d, o + struct.unpack_from('<q', d, o + 0x60)[0])[0]
        if eid > 0:
            out.setdefault(eid, (struct.unpack_from('<3f', d, o + 0x20), struct.unpack_from('<3f', d, o + 0x2C)[1]))
    return out


def tables_of(d):
    tables, o = {}, 0x10
    while True:
        _, cnt, name_off = struct.unpack_from('<iiq', d, o)
        offs = struct.unpack_from('<%dq' % (cnt - 1), d, o + 16)
        nxt = struct.unpack_from('<q', d, o + 16 + 8 * (cnt - 1))[0]
        tables[be.u16s(d, name_off)] = offs
        if nxt == 0:
            break
        o = nxt
    return tables


def rotate(rot, v):
    """MSB part rotation (degrees, X then Z then Y on a column vector: tools/sekiro-extract msb.rs
    `rotate`)."""
    sx, cx = math.sin(math.radians(rot[0])), math.cos(math.radians(rot[0]))
    sy, cy = math.sin(math.radians(rot[1])), math.cos(math.radians(rot[1]))
    sz, cz = math.sin(math.radians(rot[2])), math.cos(math.radians(rot[2]))
    v = (v[0], v[1] * cx - v[2] * sx, v[1] * sx + v[2] * cx)
    v = (v[0] * cz - v[1] * sz, v[0] * sz + v[1] * cz, v[2])
    return (v[0] * cy + v[2] * sy, v[1], -v[0] * sy + v[2] * cy)


def flver_dummy(model, ref):
    """(position, forward) of the dummy poly `ref` of an object's FLVER
    (extracted/obj/<model>.objbnd.d/<model>.flver; FLVER2 dummies right after the 0x80-byte
    header, 0x40 bytes each: position, colour, forward, reference id), in the model's space
    (relative to its parent bone, taken as the root). None without the file or the dummy."""
    path = os.path.join(bs.ROOT, 'obj', model + '.objbnd.d', model + '.flver')
    if not os.path.exists(path):
        return None
    d = open(path, 'rb').read()
    if d[:6] != b'FLVER\0':
        return None
    count = struct.unpack_from('<I', d, 0x14)[0]
    for i in range(count):
        o = 0x80 + i * 0x40
        pos = struct.unpack_from('<3f', d, o)
        fwd = struct.unpack_from('<3f', d, o + 0x10)
        if struct.unpack_from('<h', d, o + 0x1C)[0] == ref:
            return pos, fwd
    return None


def objacts(path, rows):
    """The MSB's ObjAct events (MSBS Event type 7; base @0x18, type data @0x20: ObjActEntityID,
    ObjActPartIndex, ObjActID (ObjActParam row), StateType, EventFlagID; SoulsFormats MSBS
    EventParam.ObjAct) with their object part's entity id and the ObjActParam row's reach:
    validDist (the exe's offer test FUN_1406b0c10 compares the distance with the u16 x 0.01
    x 2, so metres = validDist / 50), validPlayerAngle / validObjAngle (degrees), objAnimId, and
    the object dummy poly objDummyId's place in MSB space (the object's origin without it)."""
    d = open(path, 'rb').read()
    tables = tables_of(d)
    parts = tables['PARTS_PARAM_ST']
    models = [be.u16s(d, o + struct.unpack_from('<q', d, o)[0]) for o in tables['MODEL_PARAM_ST']]
    out = []
    for o in tables['EVENT_PARAM_ST']:
        if struct.unpack_from('<I', d, o + 0xC)[0] != 7:
            continue
        td = struct.unpack_from('<q', d, o + 0x20)[0]
        if td == 0:
            continue
        ent, pi, param, _state, _, _, flag = struct.unpack_from('<iiiBBhi', d, o + td)
        if ent <= 0 or not 0 <= pi < len(parts) or param not in rows:
            continue
        p = parts[pi]
        obj = struct.unpack_from('<i', d, p + struct.unpack_from('<q', d, p + 0x60)[0])[0]
        mi = struct.unpack_from('<i', d, p + 0x10)[0]
        pos = struct.unpack_from('<3f', d, p + 0x20)
        rot = struct.unpack_from('<3f', d, p + 0x2C)
        row = rows[param]
        dummy = flver_dummy(models[mi], row['objDummyId']) if 0 <= mi < len(models) else None
        at, fwd = pos, rotate(rot, (0.0, 0.0, 1.0))
        if dummy:
            local, f = dummy
            r = rotate(rot, local)
            at = (pos[0] + r[0], pos[1] + r[1], pos[2] + r[2])
            fwd = rotate(rot, f)
        out.append({
            'id': ent, 'object': obj, 'param': param, 'flag': flag,
            'dist': round(row['validDist'] / 50.0, 3),
            'player_angle': row['validPlayerAngle'], 'obj_angle': row['validObjAngle'],
            'anim': row['objAnimId'], 'dummy': dummy is not None,
            'at': [round(v, 3) for v in at], 'forward': [round(v, 4) for v in fwd],
        })
    return out


def main():
    rows = {r['id']: r for r in json.load(open(os.path.join(bs.ROOT, 'json', 'params', 'ObjActParam.json'), encoding='utf-8'))['rows']}
    types = bs.emedf()
    common = bs.emevd(os.path.join(bs.ROOT, 'event', 'common_func.emevd'))
    out = {}
    for f in sorted(os.listdir(os.path.join(bs.ROOT, 'event'))):
        if not (f.startswith('m') and f.endswith('_00_00.emevd')):
            continue
        mp = f[:-6]
        msb = os.path.join(bs.ROOT, 'map', 'mapstudio', mp + '.msb')
        if not os.path.exists(msb):
            continue
        evs = bs.instances(bs.emevd(os.path.join(bs.ROOT, 'event', f)), common, types)
        places = be.msb_places(msb)
        for eid, at in object_places(msb).items():
            places.setdefault(eid, at)
        regs = bs.shapes(msb)
        td = typedata(msb)
        inherited = group_gates(td)
        chars = {}
        for eid, model, npc in be.msb_parts(msb):
            if model in DUMMIES or eid not in places:
                continue
            (x, y, z), yaw = places[eid]
            think, flag, state, _ = td.get(eid, (npc, -1, 0, []))
            flag, state = inherited.get(eid, (flag, state))
            chars[eid] = {'chr': model, 'npc': npc, 'think': think, 'at': [round(x, 3), round(y, 3), round(z, 3)], 'yaw': round(yaw, 2)}
            if flag > 0:
                chars[eid]['gate'] = [flag, state == 1]
        acts = objacts(msb, rows)
        named = set().union(*(bs.names(ev) for ev in evs)) if evs else set()
        named |= {a['object'] for a in acts}
        pl, rg = {}, {}
        for eid in sorted(named):
            if eid in places and eid not in chars:
                (x, y, z), yaw = places[eid]
                pl[eid] = {'at': [round(x, 3), round(y, 3), round(z, 3)], 'yaw': round(yaw, 2)}
            if eid in regs:
                st, size, kids = regs[eid]
                rg[eid] = {'shape': st, 'size': size}
                if kids:
                    rg[eid]['parts'] = [{'shape': kst, 'size': ksize, 'at': [round(v, 3) for v in kpos], 'yaw': round(kyaw, 2)} for kst, ksize, kpos, kyaw in kids]
        out[mp] = {'map': mp, 'boss': 0, 'world': True, 'chars': chars, 'places': pl, 'regions': rg, 'events': evs, 'flags_on': [], 'objacts': acts}
        print(f'{mp}: {len(chars)} enemies, {len(evs)} events, {len(pl)} places, {len(rg)} regions, {len(acts)} ObjActs')
    path = os.path.join(bs.ROOT, 'enemies', 'world_scripts.json')
    with open(path, 'w', encoding='utf-8', newline='\n') as fh:
        json.dump(out, fh, indent=None, separators=(',', ':'), sort_keys=True)
    print(len(out), 'maps ->', path)


if __name__ == '__main__':
    main()
