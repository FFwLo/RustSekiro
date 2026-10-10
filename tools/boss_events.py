"""Boss phase events from the map scripts -> extracted/enemies/boss_events.json.

A boss's phases are driven by its map's EMEVD: events that wait for
`IF Number of Character Health Bars` (4[37], the deathblows left) on the boss and then act on
it. Examples:
- Demon of Hatred (c7020): swaps the SpEffects that set each phase's HP, posture and super armour.
- The Corrupted Monk (c5000): AI command 1 / 2, then ForceAnimationPlayback 20010.
- Isshin (c5400): at 0, EzState request 20200, his death.

Output, keyed by NpcParam row (from the MSB part that has the entity id):
  {"<npc row>": [{"bars": n, "cmp": c, "map": "m11_02_00_00", "event": id,
                  "actions": [["sp", id] | ["clear", id] | ["anim", id] | ["ezstate", id]
                              | ["ai", command, slot] | ["replan"]],
                  "needs_sp": [[SpEffect id, should have]] (IF Character Has SpEffect on the boss),
                  "needs_flags": [event flags that must be on (IF Event Flag)],
                  "sets_flags": [event flags the event turns on (SetEventFlag)],
                  "other": <count of other conditions in the same wait>}]}
cmp is the EMEDF comparison type: 0 ==, 1 !=, 2 >, 3 <, 4 >=, 5 <=.

Formats: EMEVD (Sekiro, 64-bit) per reference/SoulsFormatsNEXT Formats/EMEVD; MSB (MSBS) parts per
SoulsFormatsNEXT MSBS (entity id at the part's entity data +0, NPC param row at type data +0xC).
Instruction names: reference/DarkScript3 Resources/sekiro-common.emedf.json.
"""
import collections
import glob
import json
import os
import struct

ROOT = os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', 'extracted')


def u16s(d, o):
    e = o
    while d[e:e + 2] != b'\0\0':
        e += 2
    return d[o:e].decode('utf-16-le')


def msb_parts(path):
    """(entity id, model name, NpcParam row) of every enemy / dummy-enemy part (types 2 and 10)."""
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
    out = []
    for o in tables['PARTS_PARAM_ST']:
        _, typ, _, mi = struct.unpack_from('<qIii', d, o)
        if typ not in (2, 10):
            continue
        ent_off, td_off = struct.unpack_from('<qq', d, o + 0x60)
        eid = struct.unpack_from('<i', d, o + ent_off)[0]
        npc = struct.unpack_from('<i', d, o + td_off + 0xC)[0]
        if eid > 0:
            out.append((eid, models[mi] if mi >= 0 else '', npc))
    return out


def emevd_events(path):
    """{event id: ([(bank, id, arg bytes)], [(instr index, target, source, size)])}."""
    d = open(path, 'rb').read()
    assert d[:4] == b'EVD\0'
    h = struct.unpack_from('<16q', d, 0x10)
    ev_n, ev_o, _, ins_o, _, _, _, _, _, par_o, _, _, _, arg_o, _, _ = h
    events = {}
    for i in range(ev_n):
        eid, icount, io, pcount, po, _, _ = struct.unpack_from('<qqqqqii', d, ev_o + i * 0x30)
        ins = []
        for k in range(icount):
            bank, iid, alen, aoff, _ = struct.unpack_from('<iiqqq', d, ins_o + io + k * 0x20)
            ins.append((bank, iid, d[arg_o + aoff: arg_o + aoff + alen]))
        pars = []
        for k in range(pcount):
            ii, tgt, src, n, _ = struct.unpack_from('<qqqii', d, par_o + po + k * 0x20)
            pars.append((ii, tgt, src, n))
        events[eid] = (ins, pars)
    return events


def ints(a):
    return struct.unpack_from('<%di' % (len(a) // 4), a) if len(a) >= 4 else ()


def main():
    parts = {}
    for p in glob.glob(os.path.join(ROOT, 'map/mapstudio/*.msb')):
        for eid, model, npc in msb_parts(p):
            parts.setdefault(eid, (model, npc))
    out = collections.defaultdict(list)
    seen = set()
    for f in sorted(glob.glob(os.path.join(ROOT, 'event/m*.emevd'))):
        events = emevd_events(f)
        # Initialisations (2000[0] InitializeEvent / 2000[6] common) give parameterised events
        # their arguments.
        inits = collections.defaultdict(list)
        for ins, _ in events.values():
            for bank, iid, a in ins:
                if bank == 2000 and iid in (0, 6) and len(a) >= 8:
                    inits[struct.unpack_from('<ii', a)[1]].append(a[8:])
        for eid, (ins, pars) in events.items():
            if not any(b == 4 and i == 37 for b, i, _ in ins):
                continue
            for args in (inits.get(eid) or [None]) if pars else [None]:
                resolved = []
                for idx, (bank, iid, a) in enumerate(ins):
                    b = bytearray(a)
                    if args is not None:
                        for ii, tgt, src, n in pars:
                            if ii == idx and src + n <= len(args) and tgt + n <= len(b):
                                b[tgt:tgt + n] = args[src:src + n]
                    resolved.append((bank, iid, ints(bytes(b))))
                # Walk the event in order: a bars condition goes into its condition group; a main
                # wait (group 0, or 0[0] IF Condition Group on group 0) starts a segment, and the
                # boss actions after it belong to that segment's bars condition.
                groups = collections.defaultdict(list)
                cur, segs = None, []
                for bank, iid, v in resolved:
                    if bank in (3, 4) and v:
                        g = struct.unpack('<b', struct.pack('<B', v[0] & 0xFF))[0]
                        if (bank, iid) == (4, 37) and len(v) >= 4:
                            groups[g].append(('bars', v[1], v[2] & 0xFF, v[3]))
                        elif (bank, iid) == (4, 5) and len(v) >= 4:
                            # IF Character Has SpEffect: entity, SpEffect, Should Have (u8 at +12).
                            groups[g].append(('sp', v[1], v[2], v[3] & 0xFF))
                        elif (bank, iid) == (3, 0) and len(v) >= 2:
                            # IF Event Flag: state (u8 at +1), flag id.
                            groups[g].append(('flag', v[1], (v[0] >> 8) & 0xFF))
                        else:
                            groups[g].append(None)
                        if g == 0:
                            cur = [groups.pop(0), []]
                            segs.append(cur)
                    elif (bank, iid) == (0, 0) and v and v[0] & 0xFF == 0:
                        g = struct.unpack('<b', struct.pack('<B', (v[0] >> 16) & 0xFF))[0]
                        cur = [groups.pop(g, []), []]
                        segs.append(cur)
                    elif cur is not None:
                        cur[1].append((bank, iid, v))
                for conds, body in segs:
                    bars = [c[1:] for c in conds if c and c[0] == 'bars']
                    if len(bars) != 1:
                        continue
                    boss, cmp, n = bars[0]
                    if boss not in parts:
                        continue
                    needs = [[c[2], bool(c[3])] for c in conds if c and c[0] == 'sp' and c[1] == boss]
                    flags = [c[1] for c in conds if c and c[0] == 'flag' and c[2] == 1]
                    sets = [v[0] for bank, iid, v in body if (bank, iid) == (2003, 2) and len(v) >= 2 and v[1] & 0xFF == 1]
                    acts, other = [], len(conds) - 1 - len(needs) - len(flags)
                    for bank, iid, v in body:
                        if not v or v[0] != boss:
                            continue
                        if (bank, iid) == (2003, 18):
                            acts.append(['anim', v[1]])
                        elif (bank, iid) == (2004, 6):
                            acts.append(['ezstate', v[1]])
                        elif (bank, iid) == (2004, 17):
                            acts.append(['ai', v[1], v[2] if len(v) > 2 else 0])
                        elif (bank, iid) == (2004, 8):
                            acts.append(['sp', v[1]])
                        elif (bank, iid) == (2004, 21):
                            acts.append(['clear', v[1]])
                        elif (bank, iid) == (2004, 20):
                            acts.append(['replan'])
                    if not acts:
                        continue
                    npc = parts[boss][1]
                    key = (npc, cmp, n, json.dumps(acts))
                    if key in seen:
                        continue
                    seen.add(key)
                    mp = os.path.basename(f)[:12]
                    out[str(npc)].append({'bars': n, 'cmp': cmp, 'map': mp, 'event': eid, 'needs_sp': needs, 'needs_flags': flags, 'sets_flags': sets, 'actions': acts, 'other': other})
    path = os.path.join(ROOT, 'enemies', 'boss_events.json')
    with open(path, 'w', encoding='utf-8') as fh:
        json.dump(out, fh, indent=1, sort_keys=True)
    print(f'{sum(len(v) for v in out.values())} boss events for {len(out)} NpcParam rows -> {path}')


if __name__ == '__main__':
    main()
