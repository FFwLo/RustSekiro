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
    """Argument types per (class, instruction): DarkScript3's sekiro-common.emedf.json when the
    reference clone is there, else tools/emedf_types.json, the same types alone (what a release
    install has; regenerate it from the clone with `boss_scripts.py --emedf-table`)."""
    p = glob.glob(os.path.join(REF, 'DarkScript3', '**', 'sekiro-common.emedf.json'), recursive=True)
    if p:
        txt = re.sub(r'(?m)^\s*//.*$', '', open(p[0], encoding='utf-8').read())
        txt = re.sub(r',(\s*[}\]])', r'\1', txt)
        em = json.loads(txt)
        return {(c['index'], i['index']): [a['type'] for a in i['args']] for c in em['main_classes'] for i in c['instrs']}
    table = os.path.join(os.path.dirname(os.path.abspath(__file__)), 'emedf_types.json')
    if not os.path.exists(table):
        sys.exit(f'{table} missing (no reference/DarkScript3 clone either): no instruction argument types')
    return {tuple(int(x) for x in k.split('.')): v for k, v in json.load(open(table, encoding='utf-8')).items()}


def write_emedf_table():
    types = emedf()
    out = os.path.join(os.path.dirname(os.path.abspath(__file__)), 'emedf_types.json')
    with open(out, 'w', encoding='utf-8', newline='\n') as f:
        f.write('{\n' + ',\n'.join(f'  "{c}.{i}": {json.dumps(v)}' for (c, i), v in types.items()) + '\n}\n')
    print(f'{out}: {len(types)} instructions')


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
            # One instance per (slot, args): m10 initialises 11005800 / 11005810 / 11005900 /
            # 11005910 twice with the same slot and args. Kept once, on the reading that the game
            # keys running events by id and slot (not traced in the exe).
            seen = set()
            for slot, args in calls:
                key = (slot, bytes(args) if args is not None else None)
                if key in seen:
                    continue
                seen.add(key)
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
def brought_in(ins, parts, cast, defeats=()):
    """Parts an event enables (or shows a bar for) after a condition on the cast: a 4[*] on a cast
    member, or a 3[00] on the flag of an event that hands out a cast member's defeat (Isshin
    Ashina after Emma: m11_01 11115930 waits on her defeat event 11115900's flag)."""
    armed = False
    for bank, iid, v in ins:
        # Warped onto a cast member's dummy poly (Warp Character and Copy Floor, type 2 = onto a
        # character), whatever the wait: the Divine Dragon's five trees, c5310 2500880-84 onto
        # its dummies 50 / 53-56 (m25 12505901, after the phase flag 12505888).
        if (bank, iid) in ((2004, 41), (2004, 42)) and len(v) >= 3 and v[0] in parts and v[1] == 2 and v[2] in cast:
            yield v[0]
            continue
        if (bank == 4 and len(v) >= 2 and v[1] in cast) or ((bank, iid) == (3, 0) and len(v) >= 4 and v[3] in defeats):
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


def linked(ins, parts, boss):
    """The boss's own kind (same chr) that an event watches (a 4[*] condition on them) while it
    puts a SpEffect on the boss (2004[08] on it): the bodies of a shared bar. They stand from the start, so
    no event enables them: the Old Dragons of the Tree (m25 12505803 x15 on 2500811-825, c5300
    like the bar's 2500810: each dragon's hits feed the bar through SpEffect 3530020). Wider
    rules (any cast member, any chr, any 2004) pulled other fights' bystanders in (the Owl
    (Father)'s 1000800 through m10 11005812's 2004[05])."""
    if not any((b, i) == (2004, 8) and v and v[0] == boss for b, i, v in ins):
        return
    for b, _, v in ins:
        if b == 4 and len(v) >= 2 and v[1] in parts and v[1] != boss and parts[v[1]][0] == parts[boss][0]:
            yield v[1]


# Flags read but never set that are not world progress: the fight's own "over" state, which the
# reader would take as on. The Folding Screen Monkeys' defeat event (m20 12005800) jumps to its
# reward part on "9305 on and 9405 off" and ends on 9405 on: 9405 is set after the defeat, so a
# fresh fight has it off. (The Hirata fights' 8304 reads the same way in Lady Butterfly's event
# but the Owl (Father)'s events need it on: no rule from the shape alone.)
# The Old Dragons of the Tree (m25 12505801, bar 905300 on 2500810) start before the Divine
# Dragon: 12505949 is the shortcut 12505882 waits on (it sets 9318, and 12505885 then skips the
# Old Dragons for the Divine Dragon start). No m25 / common event sets it. 12505818-827 are the
# never-set tail of 12505885's "all 12505803-827 on" batch (12505803 sets only 803-817). Both
# start off for this row; the Divine Dragon's row 52000000 keeps them (its fight starts that way).
# The world stages 8300-8306 (common.emevd: 910 sets 8301 once Wolf holds key items 2400 / 2500 /
# 2501, the castle's ninja invasion; 911 sets 8302 on 9310, the Interior Ministry invasion; 912
# sets 8303 on "Ape 9304 defeated and 9314 not", the headless Ape still to come; 50 sets 8306 at
# a new game, the prologue, which m11_00 11100100 clears) and other bosses' defeat flags
# (9315 Emma / 11115900, 9316 Isshin Ashina / 11115920) read as on broke fights of an
# earlier stage (2026-10-13 manual test #3):
# - Genichiro on the castle top (71001000, Way of Tomoe 71100000) is before 8301: his defeat
#   event 11115800 ends on 8301 at once.
# - Emma (74000010): her defeat event 11115900 ends on 9316, Isshin Ashina's defeat.
# - The Owl in the invaded castle (50600000): 9316 / 11115900 on put Isshin Ashina's 11115930
#   into its defeated branch, and with 9316 alone off Emma's defeat would bring him in.
# - The Guardian Ape (51000000) is before 8303: its 11705823 / 824 / 826 end on it.
# - The Interior Ministry fights (Sword Saint 54000000, Tomoe 71101000) are after the prologue:
#   11120110 jumps past its set-up on 8306.
# - Lady Butterfly (50900000, after her change 50900001) is the first Hirata visit, before 8304
#   (m11_00 11105631, the second visit, whose Owl (Father) 50601010 keeps it on). (Her defeat
#   event 11005800 reads it only on its already-defeated path: the stage, not a fix.)
# - Isshin Ashina (54300000): 8300 is set only by the m11_01_54_10 variant's 11115200 (and
#   cleared by m11_01_50_60's), never by m11_01_00_00 or common. It stays on (2026-10-16): his
#   11115941 (bars 0 + Wolf's message 10 -> his 20200, Wolf's 710206) is the kneel the Todome
#   15430180 is taken from (rec_20261016_082459_boss_emma_isshin_ashina: 11115941, then 180 / 181
#   1.75 s into 710206). (Off before, when the Todome was the last mark and 20200 cut it.)
FLAGS_OFF = {
    12600050: {9405},
    53000000: {12505949, *range(12505818, 12505828)},
    71001000: {8301},
    71100000: {8301},
    74000010: {9316},
    50600000: {9316, 11115900},
    51000000: {8303},
    54000000: {8306},
    71101000: {8306},
    50900000: {8304},
    50900001: {8304},
}


def defeat_gates(evs, boss, parts):
    """The characters whose death decides the fight: the Dead/Alive checks (4[00]) in the event
    that hands out the boss defeat (2003[12]) on `boss`. They stand from the start, so no event
    enables them: the Folding Screen Monkeys' four monkeys (m20 12005800; the bar's c1000
    dummy 2000800 is the "boss")."""
    for ev in evs:
        if any((b, i) == (2003, 12) and v and v[0] == boss for b, i, v in ev['ins']):
            for b, i, v in ev['ins']:
                if (b, i) == (4, 0) and len(v) >= 2 and v[1] in parts and v[1] != boss:
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


def dead_at_start(evs, boss, cast):
    """Characters outside the cast whose death brings the boss in (an event waits on IF Character
    Dead/Alive (X, dead), then Change Character Enable State (boss, on)): a row that starts with
    the boss already in has them dead. Lady Butterfly after her change (50900001, 1000810):
    m10 11005812 enables her once 1000800 is dead, and her defeat event 11005800 needs both
    dead (2026-10-13 manual test #3)."""
    dead = set()
    for ev in evs:
        waits = []
        for b, i, v in ev['ins']:
            if (b, i) == (4, 0) and len(v) >= 3 and v[2] == 1 and v[1] not in cast and v[1] != PLAYER:
                waits.append(v[1])
            elif (b, i) == (2004, 5) and len(v) >= 2 and v[0] == boss and v[1] == 1 and waits:
                dead.update(waits)
                break
    return sorted(dead)


def arrived(ev, cast):
    """An event that brings a cast member in on a condition on a character outside the cast
    (Tomoe's 0 bars bring Isshin in: m11_02 11125830): the fight begins after it."""
    enables = any((b, i) == (2004, 5) and len(v) >= 2 and v[0] in cast and v[1] == 1 for b, i, v in ev['ins'])
    outside = any(b == 4 and len(v) >= 2 and v[1] not in cast and v[1] != PLAYER and v[1] >= 1000 for b, i, v in ev['ins'])
    return enables and outside


def other_intro(ev, cast, bosses):
    """Another fight's opening: the event enables (2004[05] on) a boss part outside this row's
    cast. The Way of Tomoe row 71100000 alone: m11_01 11115810 is Genichiro's (1110800) intro;
    run from its start it set up his fight (Wolf's 4700-4702, Genichiro enabled) once
    IF Player Standing On Hit (11[02]) was answered."""
    return any((b, i) == (2004, 5) and len(v) >= 2 and v[1] == 1 and v[0] in bosses and v[0] not in cast for b, i, v in ev['ins'])


def entry(ev, boss):
    """Where an event that brings the boss itself into the fight starts: at its last Change
    Character Enable State (boss, on) (the duel begins with the boss there: the cutscene, the
    talk before it and the world-state branches are behind it; the enable undoes the defeat
    event's disable at map load, m11_00 11105800). 0 for the others."""
    at = [k for k, (b, i, v) in enumerate(ev['ins']) if (b, i) == (2004, 5) and len(v) >= 2 and v[0] == boss and v[1] == 1]
    if at:
        return at[-1]
    # An Action Button wait (3[24]) in the event that shows the boss's bar (2003[11]) is the
    # fight's start, which the duel has already passed: the Old Dragons' 12505801 (button 9821
    # on 2500806, then cutscene 25000000, bar 905300 on 2500810 and the bar body's AI off).
    shows_bar = any((b, i) == (2003, 11) and len(v) >= 2 and v[1] == boss for b, i, v in ev['ins'])
    button = [k for k, (b, i, v) in enumerate(ev['ins']) if (b, i) == (3, 24)]
    return button[0] + 1 if shows_bar and button else 0


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


def msb_objects(path):
    """{entity id: model name} of the object parts (MSBS Part type 1)."""
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
    models = [be.u16s(d, o + struct.unpack_from('<q', d, o)[0]) for o in tables['MODEL_PARAM_ST']]
    out = {}
    for o in tables['PARTS_PARAM_ST']:
        _, typ, _, mi = struct.unpack_from('<qIii', d, o)
        if typ != 1 or mi < 0:
            continue
        eid = struct.unpack_from('<i', d, o + struct.unpack_from('<q', d, o + 0x60)[0])[0]
        if eid > 0:
            out[eid] = models[mi]
    return out


def edge_grapples(model):
    """The grapple records (type 1) of an object's e<id>.edge (docs/kb/grapple.md section 1;
    version 4, 64-byte records from 16): [hook = V1-V2 midpoint, landing = V3, variation], in
    the object's own space (FromSoftware's, unmirrored)."""
    path = os.path.join(ROOT, 'obj', model.lower() + '.objbnd.d', 'e' + model[1:] + '.edge')
    if not os.path.exists(path):
        return []
    d = open(path, 'rb').read()
    if len(d) < 16 or struct.unpack_from('<I', d, 0)[0] != 4:
        return []
    out = []
    for i in range(struct.unpack_from('<I', d, 4)[0]):
        b = 16 + i * 64
        if b + 64 > len(d) or d[b + 52] != 1:
            continue
        v1, v2, v3 = (struct.unpack_from('<3f', d, b + k * 16) for k in range(3))
        hook = [round((v1[k] + v2[k]) / 2, 4) for k in range(3)]
        out.append([hook, [round(x, 4) for x in v3], d[b + 53]])
    return out


def attached_objects(evs, objects):
    """The objects the events attach to a character (2003[58] Attach Object to Character:
    character, dummy poly, object), with their grapple edges: the Divine Dragon's trees carry o259920-922
    on dummies 600-602 (m25 12505901), whose edges are the trees' grapple points."""
    out = {}
    for ev in evs:
        for b, i, v in ev['ins']:
            if (b, i) == (2003, 58) and len(v) >= 3 and v[2] in objects:
                m = objects[v[2]]
                out[v[2]] = {'model': m, 'edges': edge_grapples(m)}
    return out


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
        objects = msb_objects(msb)
        regs = shapes(msb)
        evs = instances(emevd(f), common, types)
        bosses = {v[1] for ev in evs for b, i, v in ev['ins'] if (b, i) == (2003, 11) and len(v) >= 2 and v[1] in parts}
        for boss in sorted(bosses):
            row = parts[boss][1]
            if str(row) in out:
                continue
            cast = {boss} | set(defeat_gates(evs, boss, parts))
            for _ in range(3):
                defeats = {ev['id'] for ev in evs if any((b, i) == (2003, 12) and v and v[0] in cast for b, i, v in ev['ins'])}
                more = set(cast)
                for ev in evs:
                    if names(ev) & cast:
                        more.update(brought_in(ev['ins'], parts, cast, defeats))
                        more.update(linked(ev['ins'], parts, boss))
                if more == cast:
                    break
                cast = more
            chosen = [dict(ev, done=(arrived(ev, cast) or other_intro(ev, cast, bosses)) and not entry(ev, boss), start=entry(ev, boss)) for ev in evs if names(ev) & cast]
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
                             'flags_on': [f for f in world_flags(chosen) if f not in FLAGS_OFF.get(row, ())], 'bullets': shots, 'bullet_rows': shot_rows,
                             'sp_effects': shot_sps, 'dead_at_start': dead_at_start(chosen, boss, cast),
                             'objects': attached_objects(chosen, objects)}
    path = os.path.join(ROOT, 'enemies', 'boss_scripts.json')
    with open(path, 'w', encoding='utf-8', newline='\n') as fh:
        json.dump(out, fh, indent=None, separators=(',', ':'), sort_keys=True)
    for row, s in sorted(out.items()):
        print(row, s['map'], s['boss'], 'chars', sorted(s['chars']), 'events', len(s['events']))
    print(len(out), 'bosses ->', path)


if __name__ == '__main__':
    if '--emedf-table' in sys.argv:
        write_emedf_table()
    else:
        main()
