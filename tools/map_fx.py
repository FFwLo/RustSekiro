"""The maps' placed effects (MSB SFX / WindSFX regions) -> extracted/map_fx.json, and their
FXR ids -> extracted/map_fx_ids.txt for `python tools/fxr_extract.py --ids extracted/map_fx_ids.txt`.

MSBS POINT_PARAM_ST record (SoulsFormatsNEXT MSBS/PointParam.cs Region): name offset @0x00,
type @0x08 (5 SFX, 6 WindSFX), shape @0x10, position @0x14, rotation (degrees) @0x20, entity
data offset @0x50 (activation part index, entity id), type data offset @0x58. SFX type data:
effect id, unk, -1 x3, StartDisabled; WindSFX: effect id first.
After sekiro-rs (MIT / Apache-2.0, AKJama) docs/MAP-FX.md: the regions are placed as is (no
extra MapOffset), StartDisabled is the first-visit gate (an event's create / delete overrides it,
not run here). Its castle snowfall 839500 (entity 1114350, restored by m11_01 event 11115170).

A region is on as the first visit's event scripts leave it: start-off ones they spawn (2006[2]
Spawn Map SFX) come on, ones they delete (2006[1], the castle's later burning-state fires) go:
the instructions are walked with every event flag off, 1003[1] "skip N lines if event flag"
(args: N | state << 8, flag) followed (m11_00 11105107 spawns the snow 1108010 and skips the
invasion embers 1108020 while 8302 is off; m11_01 11115170 spawns the castle snow 1114350).
gap: the wait-until-outdoors conditions (3[2] in regions 1112150 / 1112151 ...) are not run, so
the snow also falls in the indoor rooms an arena takes in.

Usage: python tools/map_fx.py
"""
import glob
import json
import os
import struct
import sys

sys.path.insert(0, os.path.dirname(__file__))
from boss_events import emevd_events, ints  # noqa: E402

ROOT = os.path.join(os.path.dirname(__file__), '..', 'extracted')
SFX, WIND = 5, 6


def u16s(d, o):
    e = o
    while d[e:e + 2] != b'\0\0':
        e += 2
    return d[o:e].decode('utf-16-le')


def tables(d):
    out, o = {}, 0x10
    while True:
        _, cnt, name_off = struct.unpack_from('<iiq', d, o)
        out[u16s(d, name_off)] = struct.unpack_from('<%dq' % (cnt - 1), d, o + 16)
        nxt = struct.unpack_from('<q', d, o + 16 + 8 * (cnt - 1))[0]
        if nxt == 0:
            return out
        o = nxt


def regions(path):
    d = open(path, 'rb').read()
    out = []
    for o in tables(d).get('POINT_PARAM_ST', ()):
        typ = struct.unpack_from('<I', d, o + 8)[0]
        if typ not in (SFX, WIND):
            continue
        name = u16s(d, o + struct.unpack_from('<q', d, o)[0])
        pos = list(struct.unpack_from('<3f', d, o + 0x14))
        rot = list(struct.unpack_from('<3f', d, o + 0x20))
        ent = struct.unpack_from('<i', d, o + struct.unpack_from('<q', d, o + 0x50)[0] + 4)[0]
        t = o + struct.unpack_from('<q', d, o + 0x58)[0]
        fxr = struct.unpack_from('<i', d, t)[0]
        disabled = typ == SFX and struct.unpack_from('<i', d, t + 20)[0] != 0
        if fxr > 0:
            out.append({'name': name, 'entity': ent, 'fxr': fxr, 'pos': pos, 'rot': rot, 'disabled': disabled, 'wind': typ == WIND})
    return out


def first_visit_actions(emevd):
    """{entity: 'spawn' | 'delete'} of a first visit: each event walked with every event flag
    off. 1003[1] "skip N lines if event flag is <state>" skips when the state is off, 1003[2]
    "end if" ends, 1003[101] "go to label if" jumps to 1014[label], 1000[4] ends. Conditions: 3[0] IfEventFlag (group, state | type << 8 .., flag) is known (true
    when it asks for off), every other bank 3 / 4 condition unknown; 0[0] IfConditionGroup
    (group, state, target group) folds a register group (AND for positive, OR for negative). A
    main-group (0) condition that is known true goes on, known false ends the event, unknown is a
    wait. 2006[1] Delete / 2006[2] Spawn Map SFX count before such a wait (the castle's 11115820
    deletes the snow and spawns the Tomoe storm only after Genichiro's defeat); after it, only a
    spawn of what the same event deleted before it (11115170: delete the snow, wait until Wolf is
    outdoors, spawn it again), when its waits only test where Wolf is (`where`; not the Owl's smoke
    1114833, m11_01 11115873, back after an HP wait). A reachable spawn wins over a delete."""
    def sb(x):
        return x - 256 if x >= 128 else x

    def where(bank, iid, v):
        """A test of where Wolf is: 3[2] / 3[3] In/Outside Area, 3[8] Player In/Out Map, 4[5]
        Character Has SpEffect on the player (10000; the castle's indoor mark 110040)."""
        return (bank, iid) in ((3, 2), (3, 3), (3, 8)) or ((bank, iid) == (4, 5) and len(v) > 1 and v[1] == 10000)

    out = {}
    if not os.path.exists(emevd):
        return out
    for ins, _ in emevd_events(emevd).values():
        k, waited, groups, toggled, groups_kind, area_only = 0, False, {}, set(), {}, True
        while k < len(ins):
            bank, iid, a = ins[k]
            v = ints(a)
            if (bank, iid) == (1000, 4):
                break
            if (bank, iid) in ((1003, 1), (1003, 2), (1003, 101)) and len(v) >= 2:
                lines, state = v[0] & 0xff, (v[0] >> 8) & 0xff
                if state == 0:
                    if iid == 2:
                        break
                    if iid == 101:
                        k = next((j for j, x in enumerate(ins) if (x[0], x[1]) == (1014, lines)), len(ins))
                        continue
                    k += 1 + lines
                    continue
            if (bank, iid) == (1014, 0) or bank == 1014:
                k += 1
                continue
            if bank in (0, 3, 4) and v:
                g = sb(v[0] & 0xff)
                if (bank, iid) == (3, 0) and len(v) >= 2:
                    val = ((v[0] >> 8) & 0xff) == 0
                elif (bank, iid) == (0, 0):
                    tg = sb((v[0] >> 16) & 0xff)
                    rs = groups.get(tg, [])
                    if tg > 0:
                        val = False if False in rs else (None if None in rs else True)
                    else:
                        val = True if True in rs else (None if None in rs else False)
                    if val is not None and ((v[0] >> 8) & 0xff) == 0:
                        val = not val
                else:
                    val = None
                if g == 0:
                    if val is False:
                        break
                    if val is None:
                        waited = True
                        # Only a test of where Wolf is keeps the toggle respawn.
                        area_only &= where(bank, iid, v) or ((bank, iid) == (0, 0) and all(groups_kind.get(sb((v[0] >> 16) & 0xff), [True])))
                else:
                    groups.setdefault(g, []).append(val)
                    groups_kind.setdefault(g, []).append(val is not None or where(bank, iid, v))
            if bank == 2006 and iid == 2 and v and (not waited or (v[0] in toggled and area_only)):
                out[v[0]] = 'spawn'
            if bank == 2006 and iid == 1 and v and not waited:
                toggled.add(v[0])
                if out.get(v[0]) != 'spawn':
                    out[v[0]] = 'delete'
            k += 1
    return out


def main():
    result, ids = {}, set()
    for path in sorted(glob.glob(os.path.join(ROOT, 'map', 'mapstudio', 'm*_*_00_00.msb'))):
        m = os.path.basename(path)[:-4]
        r = regions(path)
        acts = first_visit_actions(os.path.join(ROOT, 'event', m + '.emevd'))
        for e in r:
            act = acts.get(e['entity'])
            e['on'] = act == 'spawn' or (not e['disabled'] and act != 'delete')
        if r:
            result[m] = r
            ids |= {e['fxr'] for e in r}
            print('%s: %d effect regions (%d start off, %d on, %d wind)' % (m, len(r), sum(e['disabled'] for e in r), sum(e['on'] for e in r), sum(e['wind'] for e in r)))
    json.dump(result, open(os.path.join(ROOT, 'map_fx.json'), 'w'), indent=1)
    open(os.path.join(ROOT, 'map_fx_ids.txt'), 'w').write('\n'.join(str(i) for i in sorted(ids)) + '\n')
    print('%d distinct effects' % len(ids))


if __name__ == '__main__':
    main()
