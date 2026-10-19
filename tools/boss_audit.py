"""Boss audit: each main boss in Sv1 against the live recording of the real fight.

Usage: python tools/boss_audit.py [--run] [--secs 240] [row ...]   (no rows: every fight)

--run first plays each fight headless (sim_tests boss_audit: real models, an unkillable Wolf who
walks up, every phase via break + deathblow) into <out>/<row>.tsv and its script trace into
<out>/<row>.err. Then, per fight, it compares with the recordings (docs/kb/recordings.md,
../memscope-data/logs/rec_*/rec.db):
- characters: the rows that fought near the boss in Sekiro, and whether Sv1 has their model,
  cloth and character data;
- attacks per phase: a000_003xxx-005xxx anims Sekiro played and Sv1 never did (and the other
  way round: only a hint, a recording shows about half to two-thirds of a boss's moves);
- deathblows: the ThrowParam rows and anims in Sekiro vs the throw anims in Sv1;
- bullets: the Bullet rows in each;
- events: the boss script's events that ran in Sekiro and not in Sv1, and the ones that touched
  the boss in Sekiro but are not in Sv1's script at all;
- arena: how far the fight spread (boss and Wolf from the boss's middle place) and the height
  range, vs Sv1's arena.
Writes docs/reports/boss-audit.md.
"""
import json
import os
import re
import sqlite3
import subprocess
import sys
from collections import Counter, defaultdict

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
LOGS = os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', '..', 'memscope-data', 'logs')
OUT = os.path.join(os.environ.get('TEMP', '.'), 'sv1_boss_audit')
EXT = os.path.join(ROOT, 'extracted')

# (name, chr, our row, recordings (suffix after rec_<date>_), the rows of the main boss in them)
FIGHTS = [
    ('Folding Screen Monkeys', 'c1260', 12600050, ['boss_monkeys', 'boss_monkeys_b'], [12600050, 12600010, 12600020, 12600000]),
    ('Corrupted Monk (illusion)', 'c5000', 50001000, ['boss_corrupted_monk'], [50001000]),
    ('True Corrupted Monk', 'c5000', 50000000, ['boss_true_monk'], [50000000]),
    ('Great Shinobi Owl', 'c5060', 50600000, ['boss_owl'], [50600000]),
    ('Owl (Father)', 'c5060', 50601010, ['boss_owl_father'], [50601010]),
    ('Gyoubu Oniwa', 'c5080', 50800000, ['boss_gyoubu', 'boss_gyoubu_b', 'boss_gyoubu_c'], [50800000]),
    ('Lady Butterfly', 'c5090', 50900000, ['boss_butterfly'], [50900000, 50900001]),
    ('Guardian Ape', 'c5100', 51000000, ['boss_guardian_ape'], [51000000]),
    ('Headless Ape', 'c5100', 51000100, ['boss_headless_ape'], [51000100, 51000010]),
    ('Old Dragons / Divine Dragon', 'c5300', 53000000, ['boss_divine_dragon'], [53000010, 53000011, 53000012, 52000000]),
    ('Way of Tomoe / Isshin, the Sword Saint', 'c7110', 71101000, ['boss_tomoe_genichiro_isshin'], [71101000, 54000000]),
    ('Emma / Isshin Ashina', 'c7400', 74000010, ['boss_emma_isshin_ashina'], [74000010, 54300000]),
    ('Demon of Hatred', 'c7020', 70200000, ['boss_demon_of_hatred'], [70200000]),
    ('Genichiro (Outskirts, first)', 'c7100', 71000000, ['boss_genichiro_first', 'boss_genichiro_first_b', 'boss_genichiro_first_c'], [71000000]),
    ('Genichiro (castle top) / Way of Tomoe', 'c7100', 71001000, ['boss_genichiro_castle', 'boss_genichiro_castle_b'], [71001000, 71100000]),
]
# Rows in the list without a fight of their own here: 50900001 (Butterfly's second row, same
# recording), 52000000 (the Divine Dragon, the Old Dragons' phase 2), 54000000 / 54300000 /
# 71100000 (second halves of the fights above).

ATTACK = re.compile(r'^a\d{3}_00[345]\d{3}$')
THROWISH = re.compile(r'^a\d{3}_(01\d{4}|02\d{4})$')


def rec_dirs(suffixes):
    out = []
    for s in suffixes:
        hits = sorted(d for d in os.listdir(LOGS) if re.match(r'rec_\d{8}_\d{6}_' + re.escape(s) + '$', d))
        out += [os.path.join(LOGS, d) for d in hits]
    return out


def row_of(who):
    m = re.search(r'c(\d{8})', who or '')
    return int(m.group(1)) if m else None


def chr_of_row(row):
    return 'c' + str(row)[:4]


def rec_data(dirs, boss_rows):
    """Per recording, joined: anims per row and phase, throws, bullets, events, places."""
    r = dict(anims=defaultdict(lambda: defaultdict(Counter)), throws=[], bullets=Counter(), events=defaultdict(set),
             rows=Counter(), near=set(), boss_pos=[], wolf_pos=[], length=0.0, ent_cmds=defaultdict(set))
    for d in dirs:
        db = sqlite3.connect(os.path.join(d, 'rec.db'))
        r['length'] += (db.execute('SELECT max(ms) FROM globals').fetchone()[0] or 0) / 1000
        # Each grab: its ThrowParam rows and the throw anims played right after it (the throw
        # row's own anim is what was playing before: Wolf's a000_7102xx walk-up to a final blow,
        # which the boss's script asks for). A deathblow is a grab Wolf's deathblow anim
        # (a2xx_5xxxxx) follows; most other grabs are the boss's own (ThrowParam xx190 / xx690).
        groups = []
        for ms, who, tid in db.execute("SELECT ms, who, id FROM fx WHERE kind='throw' ORDER BY ms"):
            if groups and ms - groups[-1][0] < 3000:
                groups[-1][2].add(tid)
            else:
                groups.append([ms, row_of(who), {tid}])
        for ms, row, tids in groups:
            after = [n for w, n in db.execute('SELECT who, name FROM anims WHERE start_ms BETWEEN ? AND ? ORDER BY start_ms', (ms - 200, ms + 2500))
                     if (w == 'wolf' and re.match(r'a2\d\d_5', n)) or (row_of(w) == row and THROWISH.match(n))]
            blow = any(re.match(r'a2\d\d_5', n) for n in after)
            r['throws'].append((blow, row, sorted(tids), ' '.join(dict.fromkeys(after))))
        # Who fought: the slots near a boss row (within 40 m at some point; the other areas'
        # characters loaded far off are left out).
        slots = {}
        for slot, who in db.execute('SELECT DISTINCT slot, who FROM frames'):
            slots.setdefault(slot, set()).add(who)
        boss_slots = {s for s, ws in slots.items() if any(row_of(w) in boss_rows for w in ws)}
        bpos = defaultdict(list)
        for tick, slot, who, x, y, z in db.execute('SELECT tick, slot, who, x, y, z FROM frames WHERE tick % 30 = 0'):
            bpos[tick].append((slot, who, x, y, z))
        for tick, rows in bpos.items():
            bs = [(x, y, z) for s, w, x, y, z in rows if row_of(w) in boss_rows]
            for s, w, x, y, z in rows:
                if w == 'wolf' or row_of(w) is None:
                    if w == 'wolf' and bs:
                        r['wolf_pos'].append((x, y, z))
                    continue
                if row_of(w) in boss_rows:
                    r['boss_pos'].append((x, y, z))
                if bs and min((x - a) ** 2 + (z - c) ** 2 for a, _, c in bs) < 40 ** 2:
                    r['near'].add(row_of(w))
        for who, name, ms in db.execute("SELECT who, name, start_ms FROM anims WHERE who != 'wolf'"):
            row = row_of(who)
            if row is None:
                continue
            r['rows'][row] += 1
            r['anims'][row][0][name] += 1
        for (bid,) in db.execute("SELECT id FROM fx WHERE kind='bullet'"):
            r['bullets'][bid] += 1
        for eid, other in db.execute("SELECT DISTINCT id, other FROM fx WHERE kind='emevd'"):
            r['events'][eid].add(other.split(' ', 1)[0])
            for ent in re.findall(r'EntityID=(\d+)', other):
                r['ent_cmds'][int(ent)].add(eid)
    return r


def ours(row):
    path = os.path.join(OUT, f'{row}.tsv')
    if not os.path.exists(path):
        return None
    o = dict(anims=defaultdict(lambda: defaultdict(Counter)), blow=defaultdict(lambda: defaultdict(set)), bullets=Counter(),
             pos=defaultdict(list), phases=0, hits=0)
    in_blow = {}
    for line in open(path, encoding='utf-8'):
        f = line.rstrip('\n').split('\t')
        kind, phase, t, who = f[0], int(f[1]), float(f[2]), f[3]
        o['phases'] = max(o['phases'], phase + 1)
        if kind == 'blow':
            in_blow[phase] = t
        elif kind == 'anim':
            if phase in in_blow and t >= in_blow[phase]:
                o['blow'][phase][who].add(f[4])
            else:
                o['anims'][who][phase][f[4]] += 1
        elif kind == 'bullet':
            o['bullets'][int(f[4])] += 1
        elif kind == 'pos':
            o['pos'][who].append(tuple(map(float, f[4:7])))
        elif kind == 'hit':
            o['hits'] += 1
        elif kind == 'won':
            o['won'] = True
    err = os.path.join(OUT, f'{row}.err')
    o['events'] = set()
    if os.path.exists(err):
        for line in open(err, encoding='utf-8', errors='replace'):
            m = re.match(r'script: event (\d+)', line)
            if m:
                o['events'].add(int(m.group(1)))
    return o


def spread(pos):
    if not pos:
        return None
    xs, ys, zs = zip(*pos)
    cx, cz = sorted(xs)[len(xs) // 2], sorted(zs)[len(zs) // 2]
    d = sorted(((x - cx) ** 2 + (z - cz) ** 2) ** 0.5 for x, _, z in pos)
    return d[int(len(d) * 0.95)], d[-1], max(ys) - min(ys)


def run(fight, secs):
    name, chr, row = fight[:3]
    os.makedirs(OUT, exist_ok=True)
    env = dict(os.environ, SHINOBI_AUDIT=os.path.join(OUT, f'{row}.tsv'), SHINOBI_ENEMIES=chr, SHINOBI_ROW=str(row),
               SHINOBI_AUDIT_SECS=str(secs), SHINOBI_SCRIPT_TRACE='1', CARGO_TARGET_DIR=os.path.join(ROOT, 'target', 'bosees'))
    with open(os.path.join(OUT, f'{row}.err'), 'w', encoding='utf-8') as err:
        p = subprocess.run(['cargo', 'test', '--profile', 'quick', '--bin', 'sv1', 'boss_audit', '--', '--ignored', '--nocapture'],
                           cwd=ROOT, env=env, stdout=subprocess.PIPE, stderr=err, text=True, encoding='utf-8', errors='replace')
    ok = 'test result: ok' in p.stdout
    print(f'{name}: {"ok" if ok else "FAILED"}', flush=True)
    return ok


def arena_of(row):
    idx = json.load(open(os.path.join(EXT, 'map_boss_arenas.json'), encoding='utf-8'))
    name = idx.get(str(row))
    src = open(os.path.join(ROOT, 'tools', 'boss_arenas.py'), encoding='utf-8').read()
    m = re.search(r'%d: \{[^}]*\'RADIUS\': \'(\d+)\'' % int((name or 'boss_0')[5:]), src)
    radius = int(m.group(1)) if m else int(re.search(r"^RADIUS = '(\d+)'", src, re.M).group(1))
    meta = {}
    if name and os.path.exists(os.path.join(EXT, f'map_{name}.json')):
        meta = json.load(open(os.path.join(EXT, f'map_{name}.json'), encoding='utf-8'))
    return name, radius, meta.get('map')


def report(fight):
    name, chr, row, recs, boss_rows = fight
    dirs = rec_dirs(recs)
    out = [f'## {name} (row {row}, {chr})', '']
    if not dirs:
        return out + ['No recording.', '']
    r = rec_data(dirs, set(boss_rows))
    o = ours(row)
    out.append(f'Recordings: {", ".join(os.path.basename(d) for d in dirs)} ({r["length"]:.0f} s, {sum(1 for t in r["throws"] if t[0] and t[1] in boss_rows)} deathblows on the boss).')
    if o is None:
        return out + ['Sv1 not run (`--run`).', '']
    out.append(f'Sv1: {o["phases"]} phases, {o["hits"]} hits on Wolf, {"won" if o.get("won") else "NOT won (the run stopped first)"}.')
    script = json.load(open(os.path.join(EXT, 'enemies', 'boss_scripts.json'), encoding='utf-8')).get(str(row), {})
    # Characters.
    ours_rows = {int(v.get('npc', 0)) for v in script.get('chars', {}).values()}
    attacked = {x for x in r['anims'] if any(ATTACK.match(a) for ph in r['anims'][x].values() for a in ph)}
    fought = sorted(x for x in r['near'] if r['rows'][x] >= 5 and (x in attacked or x in ours_rows or x in boss_rows))
    background = sorted(x for x in r['near'] if r['rows'][x] >= 5 and x not in fought)
    sv1_chrs = set(o['anims']) - {'wolf'}
    out.append('')
    out.append('**Characters** (Sekiro row: anims seen; Sv1 model / cloth / data / in the Sv1 fight):')
    for x in fought:
        c = chr_of_row(x)
        model = os.path.exists(os.path.join(EXT, f'model_{c}.bin'))
        cloth = os.path.exists(os.path.join(EXT, f'model_{c}.cloth.json'))
        data = os.path.exists(os.path.join(EXT, 'enemies', f'{c}.json'))
        out.append(f'- {x} ({c}): {r["rows"][x]} anims; model {"yes" if model else "NO"}, cloth {"yes" if cloth else "none"}, '
                   f'data {"yes" if data else "NO"}, in Sv1 fight {"yes" if c in sv1_chrs else "NO"}')
    if background:
        out.append(f'- (near the fight, never attacked: {" ".join(map(str, background))})')
    # Attacks, per character over the whole fight.
    out.append('')
    out.append('**Attacks** (a000_003xxx-005xxx; Sekiro played, Sv1 never did):')
    for x in fought:
        c = chr_of_row(x)
        mine = {a for ph in o['anims'].get(c, {}).values() for a in ph if ATTACK.match(a)}
        seen = {a for ph in r['anims'][x].values() for a in ph if ATTACK.match(a)}
        if not seen:
            continue
        miss = sorted(seen - mine)
        out.append(f'- {x}: Sekiro {len(seen)}, Sv1 {len(mine)}, both {len(seen & mine)}'
                   + (f'; Sekiro only: {" ".join(a[5:] for a in miss)}' if miss else ''))
    # Deathblows.
    out.append('')
    out.append('**Deathblows** (ThrowParam rows: the throw anims after):')
    blows = [t for t in r['throws'] if t[0] and t[1] in boss_rows]
    for i, (_, row, tids, after) in enumerate(blows):
        out.append(f'- Sekiro {i + 1} ({row}): {"/".join(map(str, tids))}: {after}')
    for ph in sorted(o['blow']):
        parts = [f'{w} {" ".join(sorted(a for a in s if THROWISH.match(a) or a.startswith("a2")))}' for w, s in sorted(o['blow'][ph].items())]
        parts = [p for p in parts if ' ' in p and p.split(' ', 1)[1]]
        out.append(f'- Sv1 {ph + 1}: ' + ('; '.join(parts) or 'no throw anims (the forced deathblow did not land)'))
    # Bullets.
    rb = {b for b in r['bullets'] if str(b)[:4] in {str(x)[:4] for x in fought}}
    ob = set(o['bullets'])
    if rb or ob:
        out.append('')
        out.append(f'**Bullets**: Sekiro {len(rb)} rows, Sv1 {len(ob)}; Sekiro only: {" ".join(map(str, sorted(rb - ob))) or "-"}; '
                   f'Sv1 only: {" ".join(map(str, sorted(ob - rb))) or "-"}')
    # Events.
    ev_ids = {e['id'] for e in script.get('events', [])}
    ents = {int(k) for k in script.get('chars', {})}
    touched = set()
    for ent in ents:
        touched |= r['ent_cmds'].get(ent, set())
    ran_rec = {e for e in r['events'] if e in ev_ids}
    out.append('')
    out.append(f'**Events**: Sv1 script has {len(ev_ids)}; {len(ran_rec)} of them ran in Sekiro; Sv1 ran {len(o["events"] & ev_ids)}.')
    miss_run = sorted(ran_rec - o['events'])
    if miss_run:
        out.append(f'- ran in Sekiro, not in Sv1: ' + ', '.join(f'{e} ({" ".join(sorted(r["events"][e]))})' for e in miss_run[:25]))
    miss_def = sorted(e for e in touched - ev_ids if e > 10000)
    if miss_def:
        out.append(f'- touched the fighters in Sekiro, not in Sv1\'s script: ' + ', '.join(f'{e} ({" ".join(sorted(r["events"][e]))})' for e in miss_def[:25]))
    # Arena.
    an, radius, amap = arena_of(row)
    bs, ws = spread(r['boss_pos']), spread(r['wolf_pos'])
    out.append('')
    line = f'**Arena**: Sv1 {an} ({amap}, radius {radius} m).'
    if bs:
        line += f' Sekiro boss from its middle: 95% within {bs[0]:.0f} m, max {bs[1]:.0f} m, height range {bs[2]:.0f} m.'
    if ws:
        line += f' Wolf: 95% within {ws[0]:.0f} m, max {ws[1]:.0f} m, height range {ws[2]:.0f} m.'
    sb = spread(o['pos'].get(chr, []))
    if sb:
        line += f' Sv1 boss: 95% within {sb[0]:.0f} m, height range {sb[2]:.0f} m.'
    out.append(line)
    out.append('')
    return out


def main():
    args = sys.argv[1:]
    secs = 240
    if '--secs' in args:
        i = args.index('--secs')
        secs = int(args[i + 1])
        del args[i:i + 2]
    do_run = '--run' in args
    rows = {int(a) for a in args if a.isdigit()}
    fights = [f for f in FIGHTS if not rows or f[2] in rows]
    if do_run:
        for f in fights:
            run(f, secs)
    lines = ['# Boss audit: Sv1 vs the live recordings', '',
             f'Made by `python tools/boss_audit.py --run --secs {secs}`: Sv1 plays each fight headless, every phase '
             f'{secs} s against an unkillable Wolf who walks up and swings every 1.5 s, and 8 s of every 20 keeps 15 m off (the boss keeps full health '
             'and posture), then a break + deathblow. Wolf never guards, heals, jumps or uses tools '
             'there, so moves the AI keeps for those show up as missing.', '']
    for f in fights:
        lines += report(f)
    path = os.path.join(ROOT, 'docs', 'reports', 'boss-audit.md')
    os.makedirs(os.path.dirname(path), exist_ok=True)
    open(path, 'w', encoding='utf-8', newline='\n').write('\n'.join(lines) + '\n')
    print(path)


if __name__ == '__main__':
    main()
