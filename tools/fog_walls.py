"""Boss fog walls from the event scripts -> extracted/enemies/fog_walls.json.

A boss room is sealed by a placed object with an effect on it while its fight is on. The maps
start two common events for it (event/common_func.emevd):

- 20005820 (end flag, object entity, SFX id, start flag): the object is enabled while the start
  flag is on and the end flag is off, with the SFX on its dummy poly (2006[4] Create
  Object-following SFX; the dummy is read from that instruction);
- 20005825 (end flag, collision entity, start flag): the same for a collision part (the arena's
  invisible boundary, 2011[1] Activate Hit).

After sekiro-rs (MIT / Apache-2.0, crates/sim/src/fogwall.rs, 5f6dc64): its castle roof reads
object 1111800 (o001401, SFX 11 on dummy 101) and collision 1114800 (h900800). Here every map
in extracted/event is read; each wall keeps the event that starts it (`from`), its flags, and
the MSB places of its object and collision part (MSBS Part: position @0x20, rotation @0x2C in
degrees, model index @0x14, entity data offset @0x60).

Usage: python tools/fog_walls.py
"""
import glob
import json
import os
import struct
import sys

sys.path.insert(0, os.path.dirname(__file__))
from boss_events import emevd_events, ints, u16s  # noqa: E402

ROOT = os.path.join(os.path.dirname(__file__), '..', 'extracted')
WALL, BOUNDARY = 20005820, 20005825


def common_dummy():
    """The dummy poly 20005820 plays its SFX on (its 2006[4] instruction's second arg)."""
    ev = emevd_events(os.path.join(ROOT, 'event', 'common_func.emevd')).get(WALL)
    for bank, iid, a in (ev[0] if ev else ()):
        if (bank, iid) == (2006, 4):
            return ints(a)[1]
    return None


def starts(path):
    """[(from event, common event id, args, parameterised)] of the 2000[6] starts in a map."""
    out = []
    for eid, (ins, pars) in emevd_events(path).items():
        subst = {p[0] for p in pars}
        for k, (bank, iid, a) in enumerate(ins):
            if (bank, iid) == (2000, 6):
                v = ints(a)
                if v and v[0] in (WALL, BOUNDARY):
                    out.append((eid, v[0], list(v[1:]), k in subst))
    return out


def msb_objects(path):
    """{entity id: (part type, model, (x, y, z), (rx, ry, rz) degrees)} of every part."""
    d = open(path, 'rb').read()
    tables = {}
    o = 0x10
    while True:
        _, cnt, name_off = struct.unpack_from('<iiq', d, o)
        offs = struct.unpack_from('<%dq' % (cnt - 1), d, o + 16)
        nxt = struct.unpack_from('<q', d, o + 16 + 8 * (cnt - 1))[0]
        tables[u16s(d, name_off)] = offs
        if nxt == 0:
            break
        o = nxt
    models = [u16s(d, o + struct.unpack_from('<q', d, o)[0]) for o in tables['MODEL_PARAM_ST']]
    out = {}
    for o in tables['PARTS_PARAM_ST']:
        typ, _, mi = struct.unpack_from('<Iii', d, o + 8)
        eid = struct.unpack_from('<i', d, o + struct.unpack_from('<q', d, o + 0x60)[0])[0]
        if eid > 0:
            out[eid] = (typ, models[mi] if 0 <= mi < len(models) else '', struct.unpack_from('<3f', d, o + 0x20), struct.unpack_from('<3f', d, o + 0x2C))

    return out


def place(parts, eid):
    if eid not in parts:
        return None
    typ, model, pos, rot = parts[eid]
    return {'entity': eid, 'type': typ, 'model': model, 'pos': [round(x, 3) for x in pos], 'rot': [round(x, 3) for x in rot]}


def main():
    dummy = common_dummy()
    out = {}
    for path in sorted(glob.glob(os.path.join(ROOT, 'event', 'm*.emevd'))):
        name = os.path.basename(path)[:-len('.emevd')]
        found = starts(path)
        if not found:
            continue
        msb = os.path.join(ROOT, 'map', 'mapstudio', name + '.msb')
        parts = msb_objects(msb) if os.path.exists(msb) else {}
        walls = []
        for frm, ev, a, param in found:
            if ev != WALL or param or len(a) < 4:
                continue
            end, obj, sfx, start = a[:4]
            coll = next((b[2][1] for b in found if b[1] == BOUNDARY and not b[3] and b[0] == frm and b[2][:1] == [end]), None)
            walls.append({'from': frm, 'start_flag': start, 'end_flag': end, 'sfx': sfx, 'dummy': dummy,
                          'object': place(parts, obj) or {'entity': obj}, 'collision': (place(parts, coll) or {'entity': coll}) if coll else None})
        skipped = sum(1 for f in found if f[3])
        if walls:
            out[name] = walls
        print(f'{name}: {len(walls)} walls' + (f' ({skipped} parameterised starts skipped)' if skipped else ''))
    dst = os.path.join(ROOT, 'enemies', 'fog_walls.json')
    json.dump(out, open(dst, 'w', encoding='utf8', newline='\n'), indent=1)
    print(dst, sum(len(v) for v in out.values()), 'walls')


if __name__ == '__main__':
    main()
