"""The maps' baked local lights (BTL banks) -> extracted/map_lights.json.

Every map folder has `<map>_NNNN.btl` banks (bank 0000 is the one a first visit loads; the others
are story states). After sekiro-rs (MIT / Apache-2.0, AKJama): crates/formats/src/btl.rs (the
file layout) and crates/game/src/map_lights.rs (how the game places them):

- header: version 2, marker 16, count, UTF-16 string bytes, 0, record size 0xE8; strings at
  0x3C, then the records;
- record: name offset @0x10, type @0x18 (0 point, 1 spot), RGB bytes @0x1D, intensity @0x20,
  cast shadows @0x27, cone angle (degrees) @0x2C, position @0x38, rotation @0x44 (radians),
  range @0x58;
- BTL positions are relative to the map's MapOffset event (MSB EVENT_PARAM_ST type 9: position
  and Y rotation in degrees), which placed parts already include: MSB = rotY(offset) * p + pos.

Output per map: the bank-0000 lights in MSB space (the game's own frame, before the arena's
X mirror). Usage: python tools/map_lights.py
"""
import glob
import json
import math
import os
import struct

ROOT = os.path.join(os.path.dirname(__file__), '..', 'extracted')


def u16s(d, o):
    e = o
    while d[e:e + 2] != b'\0\0':
        e += 2
    return d[o:e].decode('utf-16-le')


def map_offset(msb_path):
    """(position, rotation_y degrees) of the MSB's MapOffset event, or None."""
    if not os.path.exists(msb_path):
        return None
    d = open(msb_path, 'rb').read()
    o = 0x10
    while True:
        _, cnt, name_off = struct.unpack_from('<iiq', d, o)
        offs = struct.unpack_from('<%dq' % (cnt - 1), d, o + 16)
        nxt = struct.unpack_from('<q', d, o + 16 + 8 * (cnt - 1))[0]
        if u16s(d, name_off) == 'EVENT_PARAM_ST':
            for e in offs:
                if struct.unpack_from('<I', d, e + 12)[0] != 9:
                    continue
                type_off = struct.unpack_from('<q', d, e + 32)[0]
                x, y, z, ry = struct.unpack_from('<4f', d, e + type_off)
                return [x, y, z], ry
        if nxt == 0:
            return None
        o = nxt


def bank(path):
    d = open(path, 'rb').read()
    ver, marker, count, ssize, zero, rsize = struct.unpack_from('<6I', d, 0)
    if (ver, marker, zero, rsize) != (2, 16, 0, 232):
        raise ValueError('%s: unsupported BTL header' % path)
    strings, start = 60, 60 + ssize
    out = []
    for i in range(count):
        r = start + i * 232
        name = u16s(d, strings + struct.unpack_from('<I', d, r + 16)[0])
        kind = struct.unpack_from('<I', d, r + 24)[0]
        rgb = list(d[r + 29:r + 32])
        intensity = struct.unpack_from('<f', d, r + 32)[0]
        shadows = d[r + 39] != 0
        cone = struct.unpack_from('<f', d, r + 44)[0]
        pos = list(struct.unpack_from('<3f', d, r + 56))
        rot = list(struct.unpack_from('<3f', d, r + 68))
        rng = struct.unpack_from('<f', d, r + 88)[0]
        out.append({'name': name, 'kind': kind, 'color': rgb, 'intensity': intensity, 'shadows': shadows,
                    'cone': cone, 'pos': pos, 'rot': rot, 'range': rng})
    return out


def main():
    result = {}
    for path in sorted(glob.glob(os.path.join(ROOT, 'map', 'm*_*_00_00', '*_0000.btl'))):
        m = os.path.basename(os.path.dirname(path))
        lights = bank(path)
        off = map_offset(os.path.join(ROOT, 'map', 'mapstudio', m + '.msb'))
        if off:
            (tx, ty, tz), ry = off
            s, c = math.sin(math.radians(ry)), math.cos(math.radians(ry))
            for l in lights:
                x, y, z = l['pos']
                # Bevy's Quat::from_rotation_y(a) * v: x' = x c + z s, z' = -x s + z c.
                l['pos'] = [x * c + z * s + tx, y + ty, -x * s + z * c + tz]
                l['rot'][1] += math.radians(ry)
        result[m] = lights
        print('%s: %d lights (%d spot), MapOffset %s' % (m, len(lights), sum(l['kind'] == 1 for l in lights), off))
    json.dump(result, open(os.path.join(ROOT, 'map_lights.json'), 'w'), indent=1)


if __name__ == '__main__':
    main()
