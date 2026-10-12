"""Area battle music per map -> extracted/enemies/area_bgm.json (src/area_music.rs).

common.emevd event 410 (started by event 0) plays the map's MSB BGM regions by the engine's map
music state (EMEVD condition 12[2], WorldSoundManImp +0x11fc: 0 none, 1 "about to be found",
2 "about to lose you", 3 combat; docs/kb/sound.md "Area battle music"). For m11_01 (decoded)
the regions are 1112680 combat, 1112688 state 1, 1112689 state 2, 1112690 field; every map
numbers them the same way (<area><block>2680 / 2688 / 2689 / 2690), so they are read by that
suffix here (gap: 410's labels were decoded for m11_01 only; the story-raid variants 2685 /
2686 are left out). The track is FMOD event "m<SoundID>" in the area's sm<area> bank, as the
boss music.

"field" (2690, BGM_屋外フィールド_平常時) is the outdoor track: with no other track due, 410 plays
it while event flag 9100 is on and fades all music out over 1.7 s while it is off.
common_func 20004010 keeps 9100 on while the player is outside every one of up to 10 MSB
regions and off inside any; the map's event 0 passes them (InitializeCommonEvent 20004010;
their names say 屋内判定, indoor check). "indoor" lists those
regions in map space, composites (shape 6: 8 x (region index, unk), SoulsFormats MSB Shape.cs)
flattened into their children: shape 2 sphere (radius), 3 cylinder (radius, height), 5 box
(width x, depth z, height y); "yaw" in degrees. Boxes and cylinders stand on their position
(DSMapStudio RenderableProxy: the region box spans y 0..1 before scaling).

    python tools/area_bgm.py
"""

import glob
import json
import os
import struct
import sys

sys.path.insert(0, os.path.dirname(__file__))
import boss_bgm  # noqa: E402
import boss_events as be  # noqa: E402

ROOT = os.path.join(os.path.dirname(__file__), '..', 'extracted')
ROLE = {2680: 'combat', 2688: 'found_soon', 2689: 'losing', 2690: 'field'}
INDOOR_EVENT = 20004010


def msb_regions(path):
    """[(entity id, shape, pos, rot, shape data)] in the MSB's region order (MSBS Region: shape
    @0x10, position @0x14, rotation @0x20, shape data @0x48, base data 3 @0x50 entity id at +4)."""
    d = open(path, 'rb').read()
    tables = {}
    o = 0x10
    while True:
        _, cnt, name_off = struct.unpack_from('<iiq', d, o)
        offs = struct.unpack_from('<%dq' % (cnt - 1), d, o + 16)
        nxt = struct.unpack_from('<q', d, o + 16 + 8 * (cnt - 1))[0]
        tables[be.u16s(d, name_off)] = offs
        if nxt == 0:
            break
        o = nxt
    out = []
    for o in tables.get('POINT_PARAM_ST', ()):
        shape = struct.unpack_from('<I', d, o + 0x10)[0]
        pos = struct.unpack_from('<3f', d, o + 0x14)
        rot = struct.unpack_from('<3f', d, o + 0x20)
        so = struct.unpack_from('<q', d, o + 0x48)[0]
        data = ()
        if so and shape in (2, 3, 5):
            data = struct.unpack_from('<%df' % {2: 1, 3: 2, 5: 3}[shape], d, o + so)
        elif so and shape == 6:
            data = struct.unpack_from('<16i', d, o + so)[0::2]
        eid = struct.unpack_from('<i', d, o + struct.unpack_from('<q', d, o + 0x50)[0] + 4)[0]
        out.append((eid, shape, pos, rot, data))
    return out


def indoor_zones(map_id, msb_path):
    """The map's 20004010 regions (see the module doc), composites flattened."""
    path = os.path.join(ROOT, 'event', map_id + '.emevd')
    if not os.path.exists(path):
        return []
    ids = []
    for bank, iid, a in be.emevd_events(path).get(0, ([], []))[0]:
        v = be.ints(a)
        if bank == 2000 and iid == 6 and INDOOR_EVENT in v:
            ids += [x for x in v[v.index(INDOOR_EVENT) + 1:] if x > 0]
    regions = msb_regions(msb_path)
    by_id = {r[0]: r for r in regions if r[0] > 0}
    out = []

    def add(r, depth=0):
        _, shape, pos, rot, data = r
        if shape == 6 and depth < 4:
            for i in data:
                if 0 <= i < len(regions):
                    add(regions[i], depth + 1)
        elif shape in (2, 3, 5):
            size = list(data) + [0.0] * (3 - len(data))
            out.append({'shape': shape, 'pos': [round(x, 3) for x in pos], 'yaw': round(rot[1], 3), 'size': [round(x, 3) for x in size]})
        else:
            print(f'  {map_id}: indoor region shape {shape} not handled')
    for i in ids:
        if i in by_id:
            add(by_id[i])
        else:
            print(f'  {map_id}: indoor region {i} not in the MSB')
    return out


def main():
    out = {}
    for path in sorted(glob.glob(os.path.join(ROOT, 'map', 'mapstudio', 'm*_00_00.msb'))):
        map_id = os.path.basename(path)[:-4]
        regions = boss_bgm.bgm_regions(path)
        tracks = {ROLE[k % 10000]: f'm{v}' for k, v in regions.items() if k % 10000 in ROLE and k // 10000 == int(map_id[1:3] + map_id[5])}
        if 'combat' in tracks:
            out[map_id] = {'bank': f'sm{map_id[1:3]}', **tracks, 'indoor': indoor_zones(map_id, path)}
    dst = os.path.join(ROOT, 'enemies', 'area_bgm.json')
    with open(dst, 'w', encoding='utf8') as f:
        json.dump(out, f, indent=1)
    print(f'{dst}: {len(out)} maps')
    for k, v in out.items():
        print(' ', k, {x: y for x, y in v.items() if x != 'indoor'}, len(v['indoor']), 'indoor regions')


if __name__ == '__main__':
    main()
