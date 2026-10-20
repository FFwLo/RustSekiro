"""World map scripts -> extracted/enemies/world_scripts.json, run by src/enemy/world.rs.

Every event instance a base map (mXX_YY_00_00) initialises (event 50 / 0 and the common_func
events they start, tools/boss_scripts.py `instances`), with the map's enemy parts and the regions
and places its events name, for the open world (sekiro-rs's map): the same layout as a boss
script (src/enemy/script.rs ScriptDef) with "world": true, and positions raw in MSB space (no
boss frame: src/enemy/script.rs Run::place_of turns them into the world's Bevy space, X
mirrored).

Output, keyed by map: {"map", "boss": 0, "world": true,
  "chars": {entity: {"chr", "npc", "think", "at": [x, y, z], "yaw"}} (every enemy part but the
           boss rows of boss_scripts.json / npc_names.json and the c1000 / c1001 event dummies),
  "places": {entity: {"at", "yaw"}}, "regions": {entity: {"shape", "size", "parts"}},
  "events": [{"id", "slot", "rest", "ins"}], "flags_on": []}
Usage: python tools/world_scripts.py
"""
import json
import os
import struct
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import boss_events as be  # noqa: E402
import boss_scripts as bs  # noqa: E402

# Event helpers with no body of their own (the boss bars' and bullets' stand-ins).
DUMMIES = {'c1000', 'c1001'}


def thinks(path):
    """{entity: NpcThinkParam row} of the enemy parts (Part.Enemy +0x8 ThinkParamID)."""
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
            out[eid] = struct.unpack_from('<i', d, o + tdo + 0x8)[0]
    return out


def main():
    types = bs.emedf()
    common = bs.emevd(os.path.join(bs.ROOT, 'event', 'common_func.emevd'))
    bosses = set()
    p = os.path.join(bs.ROOT, 'enemies', 'boss_scripts.json')
    if os.path.exists(p):
        scripts = json.load(open(p, encoding='utf-8'))
        bosses |= {int(r) for r in scripts}
        bosses |= {c['npc'] for s in scripts.values() for c in s['chars'].values()}
    p = os.path.join(bs.ROOT, 'enemies', 'npc_names.json')
    if os.path.exists(p):
        bosses |= {int(r) for r in json.load(open(p, encoding='utf-8')).get('rows', {})}
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
        regs = bs.shapes(msb)
        think = thinks(msb)
        chars = {}
        for eid, model, npc in be.msb_parts(msb):
            if npc in bosses or model in DUMMIES or eid not in places:
                continue
            (x, y, z), yaw = places[eid]
            chars[eid] = {'chr': model, 'npc': npc, 'think': think.get(eid, npc), 'at': [round(x, 3), round(y, 3), round(z, 3)], 'yaw': round(yaw, 2)}
        named = set().union(*(bs.names(ev) for ev in evs)) if evs else set()
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
        out[mp] = {'map': mp, 'boss': 0, 'world': True, 'chars': chars, 'places': pl, 'regions': rg, 'events': evs, 'flags_on': []}
        print(f'{mp}: {len(chars)} enemies, {len(evs)} events, {len(pl)} places, {len(rg)} regions')
    path = os.path.join(bs.ROOT, 'enemies', 'world_scripts.json')
    with open(path, 'w', encoding='utf-8', newline='\n') as fh:
        json.dump(out, fh, indent=None, separators=(',', ':'), sort_keys=True)
    print(len(out), 'maps ->', path)


if __name__ == '__main__':
    main()
