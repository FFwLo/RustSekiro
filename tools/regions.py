"""Every MSB region with an entity id, per map -> extracted/enemies/regions.json.

{map: {entity id: {shape, pos, yaw, size, name}}} in map space, the shapes as tools/patrols.py
reads them (0 point, 1 circle, 2 sphere, 3 cylinder, 4 rectangle, 5 box). The AI's
IsInsideTargetRegion / SetEventMoveTarget look a region up by its entity id alone
(FUN_1406d35d0, a tree keyed by the id; 1406d.c:2135): an id no MSB has is never inside.
Usage: python tools/regions.py
"""
import glob
import json
import os
import struct
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import boss_scripts as bs  # noqa: E402
from boss_events import u16s  # noqa: E402
from patrols import tables  # noqa: E402


def regions(path):
    d = open(path, 'rb').read()
    out = {}
    for o in tables(d).get('POINT_PARAM_ST', ()):
        rid = struct.unpack_from('<i', d, o + struct.unpack_from('<q', d, o + 0x50)[0] + 4)[0]
        if rid <= 0:
            continue
        shape = struct.unpack_from('<I', d, o + 0x10)[0]
        so = struct.unpack_from('<q', d, o + 0x48)[0]
        n = {1: 1, 2: 1, 3: 2, 4: 2, 5: 3}.get(shape, 0)
        out[str(rid)] = {
            'name': u16s(d, o + struct.unpack_from('<q', d, o)[0]),
            'shape': shape if (so and n) or shape == 0 else -1,
            'pos': [round(v, 3) for v in struct.unpack_from('<3f', d, o + 0x14)],
            'yaw': round(struct.unpack_from('<3f', d, o + 0x20)[1], 3),
            'size': [round(v, 3) for v in struct.unpack_from('<%df' % n, d, o + so)] if so and n else [],
        }
    return out


def main():
    out = {}
    for f in sorted(glob.glob(os.path.join(bs.ROOT, 'map', 'mapstudio', 'm*.msb'))):
        r = regions(f)
        if r:
            out[os.path.basename(f)[:-4]] = r
            print(f'{os.path.basename(f)}: {len(r)} regions')
    path = os.path.join(bs.ROOT, 'enemies', 'regions.json')
    with open(path, 'w', encoding='utf-8', newline='\n') as f:
        json.dump(out, f, indent=1, ensure_ascii=False)


if __name__ == '__main__':
    main()
