"""Boss music from the map scripts -> extracted/enemies/boss_bgm.json.

A boss arena's EMEVD starts its music through common_func 20005830 (one track) or 20005831 (two),
InitializeCommonEvent (2000[6]) args:
  20005830: [defeat flag, arena region, start flag, BGM region, stop flag]
  20005831: [defeat flag, arena region, start flag, BGM region 1, BGM region 2, phase flag, end flag]
The common events (decoded with DarkScript3's sekiro-common.emedf.json):
  - 20005830 waits for Wolf in the arena with the start flag on, turns its BGM region on (Set Map
    Sound State + 2010[8] (region, 1)), waits for the stop flag (or the defeat flag), then turns
    every BGM off (2010[8] (-1, 256)).
  - 20005831 likewise plays region 1 unless the phase flag is already on, then when the phase
    flag goes on switches to region 2 (2010[8] (region 2, 257)); off at the defeat / end flag.
A BGM region is an MSB Sound region (PointParam type 4, SoulsFormats MSBS) with SoundType 6 (m,
BGM) and SoundID = the FMOD event "m<id>" in the area's sm<area>.fev (sm11 -> m110000001).

Output, keyed by the NpcParam row of the boss whose map script (boss_scripts.json events) sets or
reads the init's flags:
  {"<row>": {"map": "m11_00_00_00", "bank": "sm11",
             "tracks": [{"event": "m110000001", "start": flag | null, "stop": flag | null}]}}
"start" null: the fight's start (the flag is not set by the boss's own events: the arena entry
the fight begins with). "stop" null: until the defeat. A track whose "start" flag is another
track's "stop" takes over from it.
"""
import glob
import json
import os
import struct
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import boss_events as be

ROOT = be.ROOT


def bgm_regions(path):
    """{entity id: SoundID} of the MSB's Sound regions with SoundType 6 (BGM)."""
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
    out = {}
    # MSBS Region: type @8, base data 3 @0x50 (entity id at +4), type data @0x58 (Sound:
    # SoundType, SoundID).
    for o in tables.get('POINT_PARAM_ST', ()):
        if struct.unpack_from('<I', d, o + 8)[0] != 4:
            continue
        eid = struct.unpack_from('<i', d, o + struct.unpack_from('<q', d, o + 0x50)[0] + 4)[0]
        st, sid = struct.unpack_from('<ii', d, o + struct.unpack_from('<q', d, o + 0x58)[0])
        if st == 6 and eid > 0:
            out[eid] = sid
    return out


def script_flags(evs, ids):
    """(flags the events set, flags they read) over event ids `ids`."""
    sets, reads = set(), set()
    for i in ids:
        if i not in evs:
            continue
        sets.add(i)
        for bank, iid, a in evs[i][0]:
            x = be.ints(a)
            if bank == 2003 and iid == 2 and len(x) >= 2 and x[1] == 1:
                sets.add(x[0])
            elif bank in (3, 1003) and len(x) >= 2:
                reads.add(x[1])
    return sets, reads


def main():
    scripts = json.load(open(os.path.join(ROOT, 'enemies/boss_scripts.json'), encoding='utf-8'))
    out = {}
    for ev_path in sorted(glob.glob(os.path.join(ROOT, 'event/m*_00_00.emevd'))):
        m = os.path.basename(ev_path)[:-6]
        msb = os.path.join(ROOT, 'map/mapstudio/%s.msb' % m)
        if not os.path.exists(msb):
            continue
        regions = bgm_regions(msb)
        evs = be.emevd_events(ev_path)
        bank = 'sm' + m[1:3]
        inits = []
        for eid, (ins, _) in evs.items():
            for bank_, iid, a in ins:
                x = be.ints(a)
                if bank_ != 2000 or iid != 6 or len(x) < 2:
                    continue
                if x[0] == 20005830 and len(x) >= 6 and x[4] in regions:
                    _, defeat, _, start, r, stop = x[:6]
                    inits.append(([('m%09d' % regions[r], start, stop)], {start, stop, defeat}))
                elif x[0] == 20005831 and len(x) >= 8 and x[4] in regions and x[5] in regions:
                    _, defeat, _, start, r1, r2, phase, end = x[:8]
                    inits.append(([('m%09d' % regions[r1], start, phase), ('m%09d' % regions[r2], phase, end)], {start, phase, end, defeat}))
        rows = {row: s for row, s in scripts.items() if s.get('map') == m}
        for tracks, flags in inits:
            best, score = None, 0
            for row, s in rows.items():
                sets, reads = script_flags(evs, [e['id'] for e in s.get('events', [])])
                n = len(flags & (sets | reads))
                if n > score:
                    best, score = (row, sets), n
            if not best:
                print(m, 'no boss for', tracks)
                continue
            row, sets = best
            entry = out.setdefault(row, {'map': m, 'bank': bank, 'tracks': []})
            for k, (event, start, stop) in enumerate(tracks):
                # A later track whose start flag the boss's events never set can't start.
                if k > 0 and start not in sets:
                    break
                t = {'event': event, 'start': start if start in sets else None, 'stop': stop if stop in sets else None}
                # One entry per track: the Owl's (m11_01) second init of the same regions is the
                # alternative path of the same fight.
                if all(o['event'] != event for o in entry['tracks']):
                    entry['tracks'].append(t)
    for row, e in sorted(out.items()):
        print(row, e['map'], e['bank'], [(t['event'], t['start'], t['stop']) for t in e['tracks']])
    json.dump(out, open(os.path.join(ROOT, 'enemies/boss_bgm.json'), 'w', encoding='utf-8'), indent=1)


if __name__ == '__main__':
    main()
