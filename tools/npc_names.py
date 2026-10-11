"""Official English names of the bosses and mini-bosses -> extracted/enemies/npc_names.json.

The game names a fight by its health bar: 2003[11] Display Boss Health Bar / 2003[87] Display
Miniboss Health Bar (state, entity, slot, nameId) in a map's EMEVD (base mXX_YY_00_00 and the common events it calls), nameId
into msg/engus item.msgbnd NPC名.fmg (e.g. 907100 "Genichiro Ashina"). The entity is an MSB
enemy part; its NpcParam row is the key. NpcParam nameId is 0 on these rows, so the roster's
names (sekiro-extract npcs) are the Paramdex row names; data.rs encounter_list prefers these.

Output: {"rows": {"<NpcParam row>": "<name>"} (the first bar name per row, maps in name
order), "places": {"<map>": "<place name>"} (地名.fmg; gap: the id is taken as area * 100 +
block * 10 - the 11 ids are exactly the 11 map blocks, m10_00 Hirata Estate 1000 ... m25_00
Fountainhead Palace 2500 - the game's own lookup is not traced)}.
"""
import glob
import json
import os
import struct
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import boss_events as be  # noqa: E402
import boss_scripts as bs  # noqa: E402

ROOT = be.ROOT


def fmg(path):
    """FMG v2 (tools/sekiro-extract/src/fmg.rs): id -> UTF-16 text."""
    d = open(path, 'rb').read()
    out = {}
    groups = struct.unpack_from('<i', d, 0x0C)[0]
    table = struct.unpack_from('<q', d, 0x18)[0]
    for g in range(groups):
        idx, first, last = struct.unpack_from('<iii', d, 0x28 + g * 16)
        for k, i in enumerate(range(first, last + 1)):
            so = struct.unpack_from('<q', d, table + (idx + k) * 8)[0]
            if so <= 0:
                continue
            e = so
            while e + 1 < len(d) and (d[e] or d[e + 1]):
                e += 2
            s = d[so:e].decode('utf-16le')
            if s and s != '<?null?>':
                out[i] = s
    return out


def main():
    names = fmg(os.path.join(ROOT, 'msg', 'engus', 'item.msgbnd.d', 'NPC名.fmg'))
    types = bs.emedf()
    common = bs.emevd(os.path.join(ROOT, 'event', 'common_func.emevd'))
    out = {}
    for f in sorted(glob.glob(os.path.join(ROOT, 'event', 'm*_00_00.emevd'))):
        mp = os.path.basename(f)[:-6]
        msb = os.path.join(ROOT, 'map', 'mapstudio', mp + '.msb')
        if not os.path.exists(msb):
            continue
        parts = {eid: npc for eid, _model, npc in be.msb_parts(msb)}
        for ev in bs.instances(bs.emevd(f), common, types):
            for b, i, v in ev['ins']:
                if (b, i) not in ((2003, 11), (2003, 87)) or len(v) < 4 or v[0] != 1 or v[1] not in parts:
                    continue
                name = names.get(v[3])
                if name and str(parts[v[1]]) not in out:
                    out[str(parts[v[1]])] = name
    place_names = fmg(os.path.join(ROOT, 'msg', 'engus', 'item.msgbnd.d', '地名.fmg'))
    places = {}
    for f in sorted(glob.glob(os.path.join(ROOT, 'event', 'm*_00_00.emevd'))):
        mp = os.path.basename(f)[:-6]
        area, block = int(mp[1:3]), int(mp[4:6])
        if area * 100 + block * 10 in place_names:
            places[mp] = place_names[area * 100 + block * 10]
    path = os.path.join(ROOT, 'enemies', 'npc_names.json')
    with open(path, 'w', encoding='utf-8', newline='\n') as fh:
        json.dump({'rows': out, 'places': places}, fh, indent=1, ensure_ascii=False, sort_keys=True)
    print(len(out), 'named rows,', len(places), 'places ->', path)


if __name__ == '__main__':
    main()
