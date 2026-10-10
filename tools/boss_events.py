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
                  "needs_flags_off": [event flags that must be off],
                  "sets_flags": [event flags the event turns on (SetEventFlag)],
                  "message": [entity, message id] or null (IF Character Has Event Message on the
                             player 10000: Wolf's TAE 936 message, e.g. 10 in his Todome),
                  "player": [EzState ids requested of the player 10000 (Wolf's Event7102xx)],
                  "other": <count of other conditions in the same wait>}]}
More actions, on other characters of the fight (positions relative to the boss's MSB part, in
its own frame: x right, z ahead, FromSoftware's unmirrored space; yaw in degrees):
  ["enable", {"entity", "chr", "npc", "at": [x, y, z], "yaw", "summon": null | {...}}]
      (Change Character Enable State 1 on another enemy part: Genichiro's 0 bars -> Way of
      Tomoe 1110801; the Corrupted Monk's 2 bars -> her phantoms). "summon": the phantom's own
      event (another event short-warps it next to Wolf and commands its AI; the Monk's 12505970):
      {"sp": the boss SpEffect that calls it, "skip_msg": the boss event message that skips the
      first wait, "waits": [seconds], "groups": [[[x, y, z] | null]] (its warp points: one group
      per label, one entry per random flag), "areas": [{"any": [[[region, inside]]], "group"}]
      (Wolf's In/Outside Area checks, an OR of ANDs, in order; the first that holds picks the
      group, else "default"), "regions": {region: {"at", "yaw", "size": [width, depth, height]}}
      (MSB box regions, bottom centre), "start" / "go" / "stop": [[command, slot]] AI commands,
      "end_msg": its own event message that ends it}.
  ["disable"] (Change Character Enable State 0 on the boss), ["wait", seconds],
  ["warp_player", {"at": [x, y, z], "yaw"}] (the cutscene's player warp point).
A wait that follows a bars wait in the same event (only "the player is alive", a cutscene's end
or no condition)
continues its rule: Genichiro's 11115820 waits 2 s and for Wolf alive after 0 bars.
cmp is the EMEDF comparison type: 0 ==, 1 !=, 2 >, 3 <, 4 >=, 5 <=.

Formats: EMEVD (Sekiro, 64-bit) per reference/SoulsFormatsNEXT Formats/EMEVD; MSB (MSBS) parts per
SoulsFormatsNEXT MSBS (entity id at the part's entity data +0, NPC param row at type data +0xC).
Instruction names: reference/DarkScript3 Resources/sekiro-common.emedf.json.
"""
import collections
import glob
import json
import math
import os
import struct

# The player character's entity id in the map events.
PLAYER = 10000
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


def msb_places(path):
    """{entity id: ((x, y, z), yaw degrees)} of the enemy parts (types 2 / 10) and the regions."""
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
    out = {}
    # MSBS Part: position @0x20, rotation @0x2C (degrees), entity data offset @0x60.
    for o in tables['PARTS_PARAM_ST']:
        typ = struct.unpack_from('<I', d, o + 8)[0]
        if typ not in (2, 10):
            continue
        eid = struct.unpack_from('<i', d, o + struct.unpack_from('<q', d, o + 0x60)[0])[0]
        if eid > 0:
            out.setdefault(eid, (struct.unpack_from('<3f', d, o + 0x20), struct.unpack_from('<3f', d, o + 0x2C)[1]))
    # MSBS Region: shape type @0x10, position @0x14, rotation @0x20, shape data @0x48,
    # base data 3 @0x50 (entity id at +4) (SoulsFormats MSBS/PointParam.cs Region).
    for o in tables.get('POINT_PARAM_ST', ()):
        eid = struct.unpack_from('<i', d, o + struct.unpack_from('<q', d, o + 0x50)[0] + 4)[0]
        if eid > 0:
            out.setdefault(eid, (struct.unpack_from('<3f', d, o + 0x14), struct.unpack_from('<3f', d, o + 0x20)[1]))
            # Box (shape 5): width (x), depth (z), height (y) (MSB/Shape.cs Box).
            so = struct.unpack_from('<q', d, o + 0x48)[0]
            if struct.unpack_from('<I', d, o + 0x10)[0] == 5 and so:
                BOXES.setdefault(eid, [round(x, 3) for x in struct.unpack_from('<3f', d, o + so)])
    return out


# Box regions' sizes (msb_places fills it): {entity id: [width, depth, height]}.
BOXES = {}


def relative(places, boss, eid):
    """eid's position and yaw in the boss part's frame (x right, z ahead), or None."""
    if boss not in places or eid not in places:
        return None
    (bp, by), (p, y) = places[boss], places[eid]
    dx, dy, dz = p[0] - bp[0], p[1] - bp[1], p[2] - bp[2]
    r = math.radians(-by)
    # Rotation about Y by -yaw (the map's yaw turns +Z toward +X).
    lx = dx * math.cos(r) + dz * math.sin(r)
    lz = -dx * math.sin(r) + dz * math.cos(r)
    return [round(lx, 3), round(dy, 3), round(lz, 3)], round((y - by + 180.0) % 360.0 - 180.0, 2)


def summon_of(resolved, boss, ent, places):
    """The phantom event of `ent` (see the module doc), from its resolved instructions."""
    sm = {'sp': None, 'skip_msg': None, 'waits': [], 'groups': [], 'areas': [], 'default': 0, 'regions': {},
          'start': [], 'go': [], 'stop': [], 'end_msg': None}
    phase = 'start'
    # The warp choice: 3[2] In/Outside Area conditions on the player (10000) build condition
    # groups (as OR-of-AND lists), 0[0] folds one group into another, 1000[101] GOTO IF
    # (uncompiled) jumps to a label whose 2004[41] warps form one group of points (one per
    # random flag 1003[1] SKIP); falling through goes to the next label.
    cond = {}
    labels, label, rules, default = [], None, [], None

    def sbyte(x):
        return struct.unpack('<b', struct.pack('<B', x & 0xFF))[0]

    def fold(g, dnf):
        if g < 0:
            cond[g] = cond.get(g, []) + dnf
        else:
            cond[g] = [a + b for a in cond.get(g, [[]]) for b in dnf]
    for bank, iid, v in resolved:
        if (bank, iid) == (3, 2) and len(v) >= 3 and v[1] == 10000:
            fold(sbyte(v[0]), [[[v[2], (v[0] >> 8) & 0xFF == 1]]])
            continue
        if (bank, iid) == (0, 0) and v and sbyte(v[0] >> 16) in cond:
            fold(sbyte(v[0]), cond[sbyte(v[0] >> 16)])
            continue
        if (bank, iid) == (1000, 101) and v and (v[0] >> 8) & 0xFF == 1 and sbyte(v[0] >> 16) in cond:
            rules.append((cond[sbyte(v[0] >> 16)], v[0] & 0xFF))
            default = None
            continue
        if bank == 1014:
            label = iid
            if rules and default is None:
                default = iid
            continue
        if (bank, iid) == (4, 5) and len(v) >= 4 and v[1] == boss and v[3] & 0xFF == 1 and sm['sp'] is None:
            sm['sp'] = v[2]
        elif (bank, iid) in ((4, 8), (4, 21)) and len(v) >= 3 and v[1] == boss and sm['skip_msg'] is None:
            sm['skip_msg'] = v[2]
        elif (bank, iid) in ((4, 8), (4, 21)) and len(v) >= 3 and v[1] == ent:
            sm['end_msg'] = v[2]
            phase = 'stop'
        elif (bank, iid) == (1001, 0) and v and phase == 'start':
            sm['waits'].append(round(struct.unpack('<f', struct.pack('<i', v[0]))[0], 3))
        elif (bank, iid) == (2004, 41) and len(v) >= 3 and v[0] == ent:
            # One entry per random flag; a point not on the map (m25_00_50_00 warps to 0) is None.
            rel = relative(places, boss, v[2])
            if label not in labels:
                labels.append(label)
                sm['groups'].append([])
            sm['groups'][labels.index(label)].append(rel[0] if rel and v[2] != ent else None)
            phase = 'go'
        elif (bank, iid) == (2004, 17) and len(v) >= 3 and v[0] == ent:
            sm[phase].append([v[1], v[2]])
    for dnf, lab in rules:
        if lab in labels:
            sm['areas'].append({'any': dnf, 'group': labels.index(lab)})
            for conj in dnf:
                for eid, _ in conj:
                    rel = relative(places, boss, eid)
                    if rel and eid in BOXES:
                        sm['regions'][eid] = {'at': rel[0], 'yaw': rel[1], 'size': BOXES[eid]}
    if default in labels:
        sm['default'] = labels.index(default)
    return sm


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
    places = {}
    for p in glob.glob(os.path.join(ROOT, 'map/mapstudio/*.msb')):
        for eid, model, npc in msb_parts(p):
            parts.setdefault(eid, (model, npc))
        for eid, pl in msb_places(p).items():
            places.setdefault(eid, pl)
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
        all_resolved = []
        for eid, (ins, pars) in events.items():
            for args in (inits.get(eid) or [None]) if pars else [None]:
                resolved = []
                for idx, (bank, iid, a) in enumerate(ins):
                    b = bytearray(a)
                    if args is not None:
                        for ii, tgt, src, n in pars:
                            if ii == idx and src + n <= len(args) and tgt + n <= len(b):
                                b[tgt:tgt + n] = args[src:src + n]
                    resolved.append((bank, iid, ints(bytes(b))))
                all_resolved.append((eid, resolved))
        # Characters another event short-warps (2004[41]): the boss's phantoms.
        warped = {}
        for eid, resolved in all_resolved:
            for bank, iid, v in resolved:
                if (bank, iid) == (2004, 41) and v and v[0] in parts:
                    warped.setdefault(v[0], resolved)
        for eid, resolved in all_resolved:
            if not any(b == 4 and i == 37 for b, i, _ in resolved):
                continue
            if True:
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
                        elif (bank, iid) in ((4, 8), (4, 21)) and len(v) >= 3 and v[1] == PLAYER:
                            # IF Character Has Event Message (4[8]; 4[21] "New"): entity, message.
                            groups[g].append(('msg', v[1], v[2]))
                        elif (bank, iid) == (3, 0) and len(v) >= 2:
                            # IF Event Flag: state (u8 at +1), flag id.
                            groups[g].append(('flag', v[1], (v[0] >> 8) & 0xFF))
                        elif (bank, iid) == (4, 14) and len(v) >= 4 and v[1] == PLAYER and v[2] & 0xFF == 2 and v[3] == 0:
                            # IF Character HP Value: the player's HP > 0 (he is alive).
                            groups[g].append(('alive',))
                        elif (bank, iid) == (3, 31):
                            # IF Ongoing Cutscene Finished: no game state to wait for here.
                            groups[g].append(('alive',))
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
                # A later wait with no bars condition (none, or only "the player is alive")
                # continues the bars rule before it.
                merged = []
                for conds, body in segs:
                    real = [c for c in conds if not (c and c[0] == 'alive')]
                    if merged and not real and any(c and c[0] == 'bars' for c in merged[-1][0]):
                        merged[-1][1].extend(body)
                    else:
                        merged.append([real, body])
                for conds, body in merged:
                    bars = [c[1:] for c in conds if c and c[0] == 'bars']
                    if len(bars) != 1:
                        continue
                    boss, cmp, n = bars[0]
                    if boss not in parts:
                        continue
                    needs = [[c[2], bool(c[3])] for c in conds if c and c[0] == 'sp' and c[1] == boss]
                    flags = [c[1] for c in conds if c and c[0] == 'flag' and c[2] == 1]
                    flags_off = [c[1] for c in conds if c and c[0] == 'flag' and c[2] == 0]
                    sets = [v[0] for bank, iid, v in body if (bank, iid) == (2003, 2) and len(v) >= 2 and v[1] & 0xFF == 1]
                    msg = next(([c[1], c[2]] for c in conds if c and c[0] == 'msg'), None)
                    player = [v[1] for bank, iid, v in body if (bank, iid) == (2004, 6) and len(v) >= 2 and v[0] == PLAYER]
                    acts, other = [], len(conds) - 1 - len(needs) - len(flags) - len(flags_off) - (msg is not None)
                    for bank, iid, v in body:
                        if (bank, iid) == (2004, 5) and len(v) >= 2 and v[0] != boss and v[1] & 0xFF == 1 and v[0] in parts:
                            rel = relative(places, boss, v[0])
                            if rel:
                                summon = summon_of(warped[v[0]], boss, v[0], places) if v[0] in warped else None
                                acts.append(['enable', {'entity': v[0], 'chr': parts[v[0]][0], 'npc': parts[v[0]][1], 'at': rel[0], 'yaw': rel[1], 'summon': summon}])
                            continue
                        if (bank, iid) == (1001, 0) and v:
                            acts.append(['wait', round(struct.unpack('<f', struct.pack('<i', v[0]))[0], 3)])
                            continue
                        if (bank, iid) in ((2002, 4), (2002, 13)) and len(v) >= 3:
                            rel = relative(places, boss, v[2])
                            if rel:
                                acts.append(['warp_player', {'at': rel[0], 'yaw': rel[1]}])
                            continue
                        if not v or v[0] != boss:
                            continue
                        if (bank, iid) == (2004, 5) and len(v) >= 2 and v[1] & 0xFF == 0:
                            acts.append(['disable'])
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
                    if not any(x[0] != 'wait' for x in acts) and not player:
                        continue
                    npc = parts[boss][1]
                    key = (npc, cmp, n, json.dumps(acts), json.dumps(player))
                    if key in seen:
                        continue
                    seen.add(key)
                    mp = os.path.basename(f)[:12]
                    out[str(npc)].append({'bars': n, 'cmp': cmp, 'map': mp, 'event': eid, 'needs_sp': needs, 'needs_flags': flags, 'needs_flags_off': flags_off, 'sets_flags': sets, 'message': msg, 'player': player, 'actions': acts, 'other': other})
    path = os.path.join(ROOT, 'enemies', 'boss_events.json')
    with open(path, 'w', encoding='utf-8') as fh:
        json.dump(out, fh, indent=1, sort_keys=True)
    print(f'{sum(len(v) for v in out.values())} boss events for {len(out)} NpcParam rows -> {path}')


if __name__ == '__main__':
    main()
