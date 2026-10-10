"""Boss map scripts -> extracted/enemies/boss_scripts.json, run by src/enemy/script.rs.

Every character the map scripts give a boss health bar (2003[11] Display Boss Health Bar) is a
boss. For each, from its map's base EMEVD (mXX_YY_00_00; the variants are other world states)
and common_func.emevd (2000[6] common events), every event instance that names one of the
fight's characters is exported with its arguments decoded by the EMEDF (DarkScript3
sekiro-common.emedf.json types: 0 u8, 1 u16, 2 u32, 3 s8, 4 s16, 5 s32, 6 f32, 8 u32; each
aligned to its size, SoulsFormats EMEVD Instruction). The fight's characters: the boss, plus
the enemy parts its events enable, warp or show a boss bar for (Tomoe, the Monk's phantoms,
Owl's double, the Guardian Ape's mate).

Output, keyed by NpcParam row:
  {"<row>": {"map", "boss": entity,
             "chars": {entity: {"chr", "npc", "at": [x, y, z], "yaw"}},
             "places": {entity: {"at", "yaw"}} (every other entity the events name that the
                       MSB places: regions, points, parts),
             "regions": {entity: {"shape": 1 circle | 2 sphere | 3 cylinder | 4 rect | 5 box |
                                           6 composite,
                                  "size": [..] (radius / radius, height / width, depth /
                                  width, depth, height),
                                  "parts": [{"shape", "size", "at", "yaw"}] (a composite's)}},
             "events": [{"id", "slot", "rest": 0 default | 1 restart | 2 end,
                         "ins": [[bank, index, [args]]],
                         "done": it brought a cast member in on a condition on a character
                                 outside the cast (`arrived`): taken as already run (unless it
                                 brings the boss itself in: then it starts at that),
                         "start": the instruction it starts at (`entry`)}],
             "flags_on": [event flags read but never set: world progress, on],
             "bullets": {behavior id: first Bullet row} (Shoot Bullet 2003[5]: BehaviorParam
                        row, refType 1 -> its Bullet),
             "bullet_rows": {Bullet id: the row, "bulletId", "attack" (its AtkParam_Npc row +
                            "atkParamId")} (the chains: HitBulletID, intervalCreateBulletId),
             "sp_effects": {SpEffectParam id: row} (what those bullets and attacks put on)}}
Positions are in the boss part's frame (boss_events.relative: x right, z ahead, yaw degrees).
"""
import collections
import glob
import json
import os
import re
import struct
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import boss_events as be  # noqa: E402

ROOT = be.ROOT
REF = os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', '..', 'reference')
PLAYER = 10000

FMT = {0: 'B', 1: 'H', 2: 'I', 3: 'b', 4: 'h', 5: 'i', 6: 'f', 8: 'I'}


def emedf():
    p = glob.glob(os.path.join(REF, 'DarkScript3', '**', 'sekiro-common.emedf.json'), recursive=True)[0]
    txt = re.sub(r'(?m)^\s*//.*$', '', open(p, encoding='utf-8').read())
    txt = re.sub(r',(\s*[}\]])', r'\1', txt)
    em = json.loads(txt)
    return {(c['index'], i['index']): [a['type'] for a in i['args']] for c in em['main_classes'] for i in c['instrs']}


def decode(types, a):
    out, o = [], 0
    for t in types:
        f = FMT[t]
        n = struct.calcsize(f)
        o = (o + n - 1) // n * n
        if o + n > len(a):
            break
        v = struct.unpack_from('<' + f, a, o)[0]
        out.append(round(v, 6) if t == 6 else v)
        o += n
    return out


def emevd(path):
    """{event id: (rest, [(bank, index, arg bytes)], [(instr, target, source, size)])}."""
    d = open(path, 'rb').read()
    assert d[:4] == b'EVD\0'
    h = struct.unpack_from('<16q', d, 0x10)
    ev_n, ev_o, _, ins_o, _, _, _, _, _, par_o, _, _, _, arg_o, _, _ = h
    events = {}
    for i in range(ev_n):
        eid, icount, io, pcount, po, rest, _ = struct.unpack_from('<qqqqqii', d, ev_o + i * 0x30)
        ins = []
        for k in range(icount):
            bank, iid, alen, aoff, _ = struct.unpack_from('<iiqqq', d, ins_o + io + k * 0x20)
            ins.append((bank, iid, d[arg_o + aoff: arg_o + aoff + alen]))
        pars = [struct.unpack_from('<qqqii', d, par_o + po + k * 0x20)[:4] for k in range(pcount)]
        events[eid] = (rest, ins, pars)
    return events


def instances(events, common, types):
    """Every event instance (id, slot, rest, decoded instructions) the map initialises."""
    out = []
    inits = collections.defaultdict(list)
    # The preconstructor (50) runs before the constructor (0).
    for _, ins, _ in [events[k] for k in sorted(events, key=lambda k: (k != 50, k != 0))]:
        for bank, iid, a in ins:
            # Initialize Event (slot, event id, params); Initialize Common Event (event id, params).
            if bank == 2000 and iid == 0 and len(a) >= 8:
                slot, eid = struct.unpack_from('<ii', a)
                inits[eid].append((slot, a[8:]))
            elif bank == 2000 and iid == 6 and len(a) >= 4:
                inits[struct.unpack_from('<i', a)[0]].append((0, a[4:]))
    for src in (events, common):
        for eid, (rest, ins, pars) in src.items():
            calls = inits.get(eid) or ([] if src is common else [(0, None)])
            if pars and src is events and not inits.get(eid):
                continue
            for slot, args in calls:
                dec = []
                for idx, (bank, iid, a) in enumerate(ins):
                    b = bytearray(a)
                    if args is not None:
                        for ii, tgt, sr, n in pars:
                            if ii == idx and sr + n <= len(args) and tgt + n <= len(b):
                                b[tgt:tgt + n] = args[sr:sr + n]
                    dec.append([bank, iid, decode(types.get((bank, iid), [5] * (len(b) // 4)), bytes(b))])
                out.append({'id': eid, 'slot': slot, 'rest': rest, 'ins': dec})
    # In the order the map initialises them (events started on the same frame run in that order:
    # m25's 12505990 puts the Monk in her intro pose before 12505960 can wake her).
    first = {eid: k for k, eid in enumerate(inits)}
    out.sort(key=lambda ev: first.get(ev['id'], -1))
    return out


def shapes(path):
    """{entity id: (shape type, sizes, parts)} of the MSB's regions; a composite (6) lists its
    regions as (shape, sizes, position, yaw) (SoulsFormats MSBS Shape.Composite: 8 x (region
    index, unk), -1 unused)."""
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
    n = {1: 1, 2: 1, 3: 2, 4: 2, 5: 3}
    pts = tables.get('POINT_PARAM_ST', ())

    def one(o):
        st = struct.unpack_from('<I', d, o + 0x10)[0]
        so = struct.unpack_from('<q', d, o + 0x48)[0]
        if st not in n or not so:
            return None
        return (st, [round(x, 3) for x in struct.unpack_from('<%df' % n[st], d, o + so)],
                struct.unpack_from('<3f', d, o + 0x14), struct.unpack_from('<3f', d, o + 0x20)[1])

    out = {}
    for o in pts:
        eid = struct.unpack_from('<i', d, o + struct.unpack_from('<q', d, o + 0x50)[0] + 4)[0]
        st = struct.unpack_from('<I', d, o + 0x10)[0]
        so = struct.unpack_from('<q', d, o + 0x48)[0]
        if eid <= 0 or not so:
            continue
        if st == 6:
            kids = [struct.unpack_from('<i', d, o + so + 8 * k)[0] for k in range(8)]
            parts = [one(pts[k]) for k in kids if 0 <= k < len(pts)]
            out[eid] = (6, [], [x for x in parts if x])
        elif st in n:
            out[eid] = one(o)[:2] + ([],)
    return out


def rel_raw(places, boss, pos, yaw):
    """A raw MSB position / yaw in the boss part's frame (boss_events.relative)."""
    tmp = dict(places)
    tmp[-1] = (pos, yaw)
    return be.relative(tmp, boss, -1)


# Instructions that bring another character into a boss's fight: Change Character Enable State
# (2004[5], on), Issue Short Warp Request (2004[41]), Warp Character and Copy Floor (2004[42]),
# Display Boss Health Bar (2003[11], entity is the 2nd arg).
# Only after a condition on a character already in the fight (bars, message, SpEffect, death):
# world-state events that enable many characters at map load do not count.
def brought_in(ins, parts, cast):
    armed = False
    for bank, iid, v in ins:
        if bank == 4 and len(v) >= 2 and v[1] in cast:
            armed = True
            continue
        if not armed:
            continue
        if (bank, iid) == (2004, 5) and len(v) >= 2 and v[1] == 1 and v[0] in parts:
            yield v[0]
        elif (bank, iid) in ((2004, 41), (2004, 42)) and v and v[0] in parts:
            yield v[0]
        elif (bank, iid) == (2003, 11) and len(v) >= 2 and v[1] in parts:
            yield v[1]


def world_flags(evs):
    """Event flags the script reads but never sets: world progress (the fight is open, e.g.
    Owl's 8304), taken as on. Flags it sets (the boss defeated 9304, its phase flags) start off."""
    read, written = set(), set()
    for ev in evs:
        written.add(ev['id'])
        for b, i, v in ev['ins']:
            if (b, i) == (3, 0) and len(v) >= 4 and v[2] == 0:
                read.add(v[3])
            elif (b, i) in ((3, 1), (1003, 3), (1003, 103)) and len(v) >= 5 and v[2] == 0:
                read.update(range(v[3], v[4] + 1))
            elif (b, i) == (3, 10) and len(v) >= 4 and v[1] == 0:
                read.update(range(v[2], v[3] + 1))
            elif (b, i) == (1003, 0) and len(v) >= 3 and v[1] == 0:
                read.add(v[2])
            elif (b, i) in ((1003, 1), (1003, 2), (1003, 101)) and len(v) >= 4 and v[2] == 0:
                read.add(v[3])
            elif (b, i) == (2003, 2) and v:
                written.add(v[0])
            elif (b, i) in ((2003, 17), (2003, 22)) and len(v) >= 2:
                written.update(range(v[0], v[1] + 1))
    return sorted(read - written)


def arrived(ev, cast):
    """An event that brings a cast member in on a condition on a character outside the cast
    (Tomoe's 0 bars bring Isshin in: m11_02 11125830): the fight begins after it."""
    enables = any((b, i) == (2004, 5) and len(v) >= 2 and v[0] in cast and v[1] == 1 for b, i, v in ev['ins'])
    outside = any(b == 4 and len(v) >= 2 and v[1] not in cast and v[1] != PLAYER and v[1] >= 1000 for b, i, v in ev['ins'])
    return enables and outside


def entry(ev, boss):
    """Where an event that brings the boss itself into the fight starts: at its last Change
    Character Enable State (boss, on) (the duel begins with the boss there: the cutscene, the
    talk before it and the world-state branches are behind it; the enable undoes the defeat
    event's disable at map load, m11_00 11105800). 0 for the others."""
    at = [k for k, (b, i, v) in enumerate(ev['ins']) if (b, i) == (2004, 5) and len(v) >= 2 and v[0] == boss and v[1] == 1]
    return at[-1] if at else 0


def param_rows(name):
    d = json.load(open(os.path.join(ROOT, 'json', 'params', name + '.json'), encoding='utf-8'))
    rows = d['rows'] if isinstance(d, dict) else d
    return {r['id']: r for r in rows}


PARAMS = {}


def script_bullets(evs):
    """The bullets the events shoot (Shoot Bullet 2003[5]: owner, source, dummy, behavior id):
    BehaviorParam row (refType 1) -> its Bullet row and the rows it chains to, each with its
    AtkParam_Npc row (the exporter's bulletRows shape, src/data.rs BulletSpec)."""
    for n in ('BehaviorParam', 'Bullet', 'AtkParam_Npc'):
        if n not in PARAMS:
            PARAMS[n] = param_rows(n)
    beh, bul, atk = PARAMS['BehaviorParam'], PARAMS['Bullet'], PARAMS['AtkParam_Npc']
    first, rows = {}, {}
    for ev in evs:
        for b, i, v in ev['ins']:
            if (b, i) != (2003, 5) or len(v) < 4 or v[3] not in beh or beh[v[3]].get('refType') != 1:
                continue
            first[v[3]] = beh[v[3]]['refId']
            todo = [beh[v[3]]['refId']]
            while todo:
                bid = todo.pop()
                if bid <= 0 or str(bid) in rows or bid not in bul:
                    continue
                r = dict(bul[bid])
                r.pop('id', None)
                r['bulletId'] = bid
                a = atk.get(r.get('atkId_Bullet', -1))
                if a:
                    a = dict(a)
                    a.pop('id', None)
                    a['atkParamId'] = r['atkId_Bullet']
                    r['attack'] = a
                rows[str(bid)] = r
                todo += [r.get('HitBulletID', -1), r.get('intervalCreateBulletId', -1)]
    if 'SpEffectParam' not in PARAMS:
        PARAMS['SpEffectParam'] = param_rows('SpEffectParam')
    keys = ['spEffectId%d' % i for i in range(5)]
    ids = {r.get(k, -1) for r in rows.values() for k in keys} | {r['attack'].get(k, -1) for r in rows.values() if 'attack' in r for k in keys}
    sps = {}
    for i in sorted(x for x in ids if x > 0 and x in PARAMS['SpEffectParam']):
        r = dict(PARAMS['SpEffectParam'][i])
        r.pop('id', None)
        sps[str(i)] = r
    return first, rows, sps


def names(ev):
    """Entity-like ints the event uses (not its initialisations of other events)."""
    return {x for b, _, v in ev['ins'] if b != 2000 for x in v if isinstance(x, int) and x >= 1000}


def main():
    types = emedf()
    common = {k: v for k, v in emevd(os.path.join(ROOT, 'event', 'common_func.emevd')).items()}
    out = {}
    for f in sorted(glob.glob(os.path.join(ROOT, 'event', 'm*_00_00.emevd'))):
        mp = os.path.basename(f)[:-6]
        msb = os.path.join(ROOT, 'map', 'mapstudio', mp + '.msb')
        if not os.path.exists(msb):
            continue
        parts = {eid: (model, npc) for eid, model, npc in be.msb_parts(msb)}
        places = be.msb_places(msb)
        regs = shapes(msb)
        evs = instances(emevd(f), common, types)
        bosses = {v[1] for ev in evs for b, i, v in ev['ins'] if (b, i) == (2003, 11) and len(v) >= 2 and v[1] in parts}
        for boss in sorted(bosses):
            row = parts[boss][1]
            if str(row) in out:
                continue
            cast = {boss}
            for _ in range(3):
                more = set(cast)
                for ev in evs:
                    if names(ev) & cast:
                        more.update(brought_in(ev['ins'], parts, cast))
                if more == cast:
                    break
                cast = more
            chosen = [dict(ev, done=arrived(ev, cast) and not entry(ev, boss), start=entry(ev, boss)) for ev in evs if names(ev) & cast]
            named = set().union(*(names(ev) for ev in chosen)) if chosen else set()
            chars, pl, rg = {}, {}, {}
            for eid in sorted(named):
                rel = be.relative(places, boss, eid)
                if not rel:
                    continue
                if eid in cast:
                    chars[eid] = {'chr': parts[eid][0], 'npc': parts[eid][1], 'at': rel[0], 'yaw': rel[1]}
                else:
                    pl[eid] = {'at': rel[0], 'yaw': rel[1]}
                if eid in regs:
                    st, size, kids = regs[eid]
                    rg[eid] = {'shape': st, 'size': size}
                    if kids:
                        rg[eid]['parts'] = []
                        for kst, ksize, kpos, kyaw in kids:
                            at, yaw = rel_raw(places, boss, kpos, kyaw)
                            rg[eid]['parts'].append({'shape': kst, 'size': ksize, 'at': at, 'yaw': yaw})
            shots, shot_rows, shot_sps = script_bullets(chosen)
            out[str(row)] = {'map': mp, 'boss': boss, 'chars': chars, 'places': pl, 'regions': rg, 'events': chosen,
                             'flags_on': world_flags(chosen), 'bullets': shots, 'bullet_rows': shot_rows,
                             'sp_effects': shot_sps}
    path = os.path.join(ROOT, 'enemies', 'boss_scripts.json')
    with open(path, 'w', encoding='utf-8', newline='\n') as fh:
        json.dump(out, fh, indent=None, separators=(',', ':'), sort_keys=True)
    for row, s in sorted(out.items()):
        print(row, s['map'], s['boss'], 'chars', sorted(s['chars']), 'events', len(s['events']))
    print(len(out), 'bosses ->', path)


if __name__ == '__main__':
    main()
