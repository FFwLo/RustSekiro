"""Coverage table: how far Sv1 is proven to match Sekiro, per character and per system.

Writes docs/kb/coverage.md. Everything above the marker line in that file is the hand-written
intro and is kept; everything below it is regenerated.

    python tools/coverage.py

Inputs (read only):
  - the repo this script sits in: src/ (code, sim tests), docs/kb/ (notes);
  - $SHINOBI_ROOT/extracted (default <repo>): chr/, model_c*.bin,
    anim_c*.bin, hks_src/, script/*.luabnd.d (AI bytecode), ai_src/ (decompiled AI),
    enemies/ (roster, npc_names, boss_scripts), map_boss_arenas.json, json/params/NpcThinkParam,
    ref_frames/, emu/, event/ + map/mapstudio/ (boss vs mini-boss health bars, via
    tools/boss_events.py and tools/boss_scripts.py);
  - $SHINOBI_REC_DIR (default <work>/memscope-data/logs): the live recordings of the
    real game (manifest.json), replay_all.md, enemies_all.md, enemies_pressure.md, the
    enemy_ours_c<row>.csv and pressure/ours_c<chr>_<row>.csv replays. Missing: shown as "?".

Prints names, ids and counts only: no game bytes, no decompiled code.
"""
import collections
import glob
import json
import os
import re
import sys
import time

REPO = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SHINOBI_ROOT = os.environ.get('SHINOBI_ROOT', '<repo>')
EX = os.path.join(SHINOBI_ROOT, 'extracted')
REC = os.environ.get('SHINOBI_REC_DIR', '<work>/memscope-data/logs')
OUT = os.path.join(REPO, 'docs', 'kb', 'coverage.md')
MARKER = '<!-- Generated below by tools/coverage.py: edit the intro above, not the tables. -->'

LEVELS = ['none', 'data loaded', 'rules ported', 'checked against the exe', 'seen side by side']

# ref_frames file name token -> chr (docs/kb/visuals.md: detail_{wolf,gen}_* are Wolf and the
# General; the owl shots are named owl).
REF_FRAME_CHR = {'wolf': 'c0000', 'gen': 'c1020', 'owl': 'c5060'}

CHR_RE = re.compile(r'\bc(\d{4})\b')

# chr ids with no NpcParam name whose identity the repo states (source in brackets).
KNOWN = {'c0000': 'Wolf, the player (src/player.rs)'}
KNOWN_NOTES = {'c9997': 'shared NPC HKS script, not a character (enemy.rs ports its guard and turn parts)'}


def read(path, binary=False):
    try:
        with open(path, 'rb') as f:
            d = f.read()
    except OSError:
        return None
    return d if binary else d.decode('utf-8', 'replace')


def load_json(path):
    t = read(path)
    if t is None:
        return None
    try:
        return json.loads(t)
    except ValueError:
        return None


def rel(p):
    return os.path.relpath(p, REPO).replace('\\', '/')


def cell(s):
    return str(s).replace('|', '/').replace('\n', ' ')


# ---------------------------------------------------------------- game data

def chr_ids():
    """Every cNNNN the extracted data names, with where it was seen."""
    seen = collections.defaultdict(set)
    for f in os.listdir(os.path.join(EX, 'chr')) if os.path.isdir(os.path.join(EX, 'chr')) else []:
        m = re.match(r'(c\d{4})', f)
        if m:
            seen[m.group(1)].add('chr')
    for f in glob.glob(os.path.join(EX, 'model_c*.bin')):
        seen[os.path.basename(f)[6:11]].add('model')
    for f in glob.glob(os.path.join(EX, 'anim_c*.bin')):
        for c in re.findall(r'c\d{4}', os.path.basename(f)):
            seen[c].add('anim')
    for f in glob.glob(os.path.join(EX, 'hks_src', 'c*.lua')):
        m = re.match(r'(c\d{4})', os.path.basename(f))
        if m:
            seen[m.group(1)].add('hks')
    for f in glob.glob(os.path.join(EX, 'enemies', 'c*.json')):
        seen[os.path.basename(f)[:5]].add('enemies')
    return seen


def chr_bundles():
    kinds = collections.defaultdict(set)
    d = os.path.join(EX, 'chr')
    letter = {'chrbnd': 'C', 'anibnd': 'A', 'behbnd': 'B', 'texbnd': 'T'}
    for f in os.listdir(d) if os.path.isdir(d) else []:
        m = re.match(r'(c\d{4})[^.]*\.(chrbnd|anibnd|behbnd|texbnd)$', f)
        if m:
            kinds[m.group(1)].add(letter[m.group(2)])
    return kinds


def battle_goals():
    """NpcThinkParam row -> (battleGoalID, logicId)."""
    d = load_json(os.path.join(EX, 'json', 'params', 'NpcThinkParam.json'))
    out = {}
    for r in (d or {}).get('rows', []):
        out[r.get('id')] = (r.get('battleGoalID', 0), r.get('logicId', 0))
    return out


def ai_files():
    """Battle goal ids with game bytecode (script/*.luabnd.d) and with decompiled source (ai_src)."""
    byte, src = set(), set()
    for f in glob.glob(os.path.join(EX, 'script', '*.luabnd.d', '*_battle.lua')):
        byte.add(int(os.path.basename(f)[:6]))
    for f in glob.glob(os.path.join(EX, 'ai_src', '**', '*_battle.lua'), recursive=True):
        b = os.path.basename(f)[:6]
        if b.isdigit():
            src.add(int(b))
    return byte, src


def health_bars():
    """NpcParam row -> 'boss' (2003[11] Display Boss Health Bar) or 'mini-boss' (2003[87]), from
    the maps' EMEVD the way tools/npc_names.py reads them. None when the helpers cannot run."""
    try:
        sys.path.insert(0, os.path.join(REPO, 'tools'))
        sys.dont_write_bytecode = True  # no tools/__pycache__ left behind
        import boss_events as be
        import boss_scripts as bs
        be.ROOT = EX
        bs.ROOT = EX
        types = bs.emedf()
        common = bs.emevd(os.path.join(EX, 'event', 'common_func.emevd'))
        out = {}
        for f in sorted(glob.glob(os.path.join(EX, 'event', 'm*_00_00.emevd'))):
            mp = os.path.basename(f)[:-6]
            msb = os.path.join(EX, 'map', 'mapstudio', mp + '.msb')
            if not os.path.exists(msb):
                continue
            parts = {eid: npc for eid, _model, npc in be.msb_parts(msb)}
            for ev in bs.instances(bs.emevd(f), common, types):
                for b, i, v in ev['ins']:
                    if (b, i) in ((2003, 11), (2003, 87)) and len(v) >= 4 and v[0] == 1 and v[1] in parts:
                        kind = 'boss' if i == 11 else 'mini-boss'
                        row = parts[v[1]]
                        if out.get(row) != 'boss':
                            out[row] = kind
        return out
    except Exception as e:  # noqa: BLE001 - any failure: fall back to "bar" without the kind
        print('health bars: EMEVD read failed (%s); bosses marked "bar" only' % e)
        return None


# ---------------------------------------------------------------- code and tests

def src_files():
    out = {}
    for f in glob.glob(os.path.join(REPO, 'src', '**', '*.*'), recursive=True):
        if f.endswith(('.rs', '.wgsl')):
            out[rel(f)] = read(f)
    return out


TEST_RE = re.compile(r'#\[test\]((?:\s*#\[[^\]]*\])*)\s*(?:pub(?:\([a-z]+\))?\s+)?fn\s+(\w+)\s*\(')


def tests(files):
    """Every #[test]: name, file, body, ignored, reads extracted/emu."""
    out = []
    for path, text in files.items():
        if not path.endswith('.rs'):
            continue
        for m in TEST_RE.finditer(text):
            i = text.find('{', m.end())
            depth, j = 0, i
            while j < len(text):
                ch = text[j]
                if ch == '{':
                    depth += 1
                elif ch == '}':
                    depth -= 1
                    if depth == 0:
                        break
                j += 1
            body = text[i:j + 1]
            out.append({
                'name': m.group(2),
                'file': path,
                'body': body,
                'ignored': 'ignore' in m.group(1),
                'sim': path.startswith('src/sim_tests/'),
                'emu': 'extracted/emu/' in body or m.group(2).endswith('_matches_the_exe'),
            })
    return out


# ---------------------------------------------------------------- recordings

def recordings():
    """Recording folder -> {name, note, cats}; empty when the folder is not on this PC."""
    out = {}
    if not os.path.isdir(REC):
        return out
    for d in sorted(os.listdir(REC)):
        if not d.startswith('rec_'):
            continue
        m = load_json(os.path.join(REC, d, 'manifest.json')) or {}
        out[d] = {'name': m.get('name', ''), 'note': m.get('note', ''), 'cats': set(m.get('cats') or [])}
    # recordings.md (the repo's list) carries the notes of the v1 recordings too
    t = read(os.path.join(REPO, 'docs', 'kb', 'recordings.md')) or ''
    for line in t.splitlines():
        c = [x.strip() for x in line.split('|')]
        if len(c) > 5 and c[1].startswith('rec_'):
            r = out.setdefault(c[1], {'name': c[1], 'note': '', 'cats': set()})
            r['note'] = r['note'] or c[3]
            r['cats'] |= {x for x in c[4].split(',') if x}
    return out


def replay_all():
    """replay_all.md: recording -> (game, ours, matched) Wolf events."""
    t = read(os.path.join(REC, 'replay_all.md'))
    if t is None:
        return None
    out = {}
    for m in re.finditer(r'^(rec_\S+): game (\d+), ours (\d+), matched (\d+)', t, re.M):
        out[m.group(1)] = tuple(int(x) for x in m.group(2, 3, 4))
    return out


def replay_events():
    """replay_all.md event table: W_ event -> (game, ours, matched); it lists the most-missed events only."""
    t = read(os.path.join(REC, 'replay_all.md'))
    if t is None:
        return None
    return {m.group(1): tuple(int(x) for x in m.group(2, 3, 4))
            for m in re.finditer(r'^(W_\w+) \| (\d+) \| (\d+) \| (\d+) \|', t, re.M)}


def enemies_all():
    """enemies_all.md: chr -> number of recordings its kind was profiled against."""
    t = read(os.path.join(REC, 'enemies_all.md'))
    if t is None:
        return None
    return {m.group(1): int(m.group(2)) for m in re.finditer(r'^## (c\d{4}) - (\d+) recording', t, re.M)}


def enemies_pressure():
    t = read(os.path.join(REC, 'enemies_pressure.md'))
    if t is None:
        return None
    return {m.group(1) for m in re.finditer(r'^(c\d{4}) \| \d+ \|', t, re.M)}


def replay_csvs():
    """chr or NpcParam row -> recordings with an enemy replay csv; pressure replays by chr."""
    rows = collections.defaultdict(set)
    for f in glob.glob(os.path.join(REC, 'rec_*', 'enemy_ours_c*.csv')):
        row = int(re.search(r'enemy_ours_c(\d+)\.csv', f).group(1))
        rows[row].add(os.path.basename(os.path.dirname(f)))
    press = collections.defaultdict(set)
    for f in glob.glob(os.path.join(REC, 'pressure', 'ours_c*_*.csv')):
        m = re.search(r'ours_(c\d{4})_(\d+)\.csv', f)
        press[m.group(1)].add(int(m.group(2)))
    return rows, press


# ---------------------------------------------------------------- characters

def data_checks(all_tests):
    """chr -> the 'Data vs exe' cell: how many of its param rows (attacks, SpEffects, bullets, its
    params tables; Wolf's also behaviours and tools) the test character_rows_match_the_exe
    compares, field by field, with the game's loaded tables (extracted/emu/params_rows.json,
    tools/emu-cases params). Counts only rows the dump holds; '-' without the dump or the test."""
    if not any(t['name'] == 'character_rows_match_the_exe' and not t['ignored'] for t in all_tests):
        return {}
    dump = load_json(os.path.join(EX, 'emu', 'params_rows.json'))
    if not dump:
        return {}
    have = {n: set(t['rows']) for n, t in dump['tables'].items()}

    def count(d, pc):
        atk = 'AtkParam_Pc' if pc else 'AtkParam_Npc'
        n = collections.Counter()
        for a in (d.get('attacks') or {}).values():
            n[atk] += str(a.get('atkParamId')) in have.get(atk, ())
        for i in d.get('spEffects') or {}:
            n['SpEffectParam'] += i in have['SpEffectParam']
        for g in ('bullets', 'bulletRows'):
            for x in (d.get(g) or {}).values():
                n['Bullet'] += str(x.get('bulletId')) in have['Bullet']
                if isinstance(x.get('attack'), dict):
                    n[atk] += str(x['attack'].get('atkParamId')) in have.get(atk, ())
        for i in d.get('behaviors') or {}:
            n['BehaviorParam_PC'] += i in have['BehaviorParam_PC']
        for x in d.get('prosthetics') or []:
            n['EquipParamWeapon'] += str(x.get('id')) in have['EquipParamWeapon']
        for t, rows in (d.get('params') or {}).items():
            n[t] += len([i for i in rows if i in have.get(t, ())])
        return n

    out = {}
    cd = load_json(os.path.join(EX, 'combat_data.json')) or {}
    srcs = [('c0000', cd.get('player') or {}, True)] + [
        (os.path.basename(f)[:5], load_json(f) or {}, False) for f in glob.glob(os.path.join(EX, 'enemies', 'c[0-9][0-9][0-9][0-9].json'))]
    for c, d, pc in srcs:
        n = count(d, pc)
        if sum(n.values()):
            top = ', '.join('%s %d' % (k, v) for k, v in n.most_common(3))
            out[c] = '%d rows the same (%s%s)' % (sum(n.values()), top, ', ...' if len(n) > 3 else '')
    return out


def characters(files, all_tests, kb):
    seen = chr_ids()
    bundles = chr_bundles()
    roster = load_json(os.path.join(EX, 'enemies', 'roster.json')) or []
    by_chr = {e['chr']: e for e in roster}
    names = (load_json(os.path.join(EX, 'enemies', 'npc_names.json')) or {}).get('rows', {})
    scripts = load_json(os.path.join(EX, 'enemies', 'boss_scripts.json')) or {}
    arenas = load_json(os.path.join(EX, 'map_boss_arenas.json')) or {}
    goals = battle_goals()
    byte, decomp = ai_files()
    bars = health_bars()
    recs = recordings()
    rp_rows, rp_press = replay_csvs() if os.path.isdir(REC) else ({}, {})
    prof = enemies_all()
    press = enemies_pressure()
    ref = sorted(os.listdir(os.path.join(EX, 'ref_frames'))) if os.path.isdir(os.path.join(EX, 'ref_frames')) else []
    clips = read(os.path.join(REPO, 'docs', 'kb', 'clips.md')) or ''
    data_exe = data_checks(all_tests)

    row_chr = collections.defaultdict(set)
    for e in roster:
        for p in e['placements']:
            row_chr[p['npc']].add(e['chr'])
    unique_row = {r: next(iter(c)) for r, c in row_chr.items() if len(c) == 1}

    casts = collections.defaultdict(list)  # chr -> boss rows whose fight it is in
    for boss, s in scripts.items():
        for ch in sorted({x['chr'] for x in s.get('chars', {}).values()}):
            casts[ch].append(boss)

    # where each chr id is named in src (not tests) and in sim tests
    named = collections.defaultdict(set)
    for path, text in files.items():
        if path.startswith('src/sim_tests/'):
            continue
        for c in set('c' + x for x in CHR_RE.findall(text)):
            named[c].add(path.split('/', 1)[1])
    sim = [t for t in all_tests if t['sim'] and not t['ignored']]
    test_chr = collections.defaultdict(list)
    default_c1020 = []
    for t in sim:
        hit = set('c' + x for x in CHR_RE.findall(t['body']))
        for n in re.findall(r'\b(\d{8})\b', t['body']):
            if int(n) in unique_row:
                hit.add(unique_row[int(n)])
        for c in hit:
            test_chr[c].append(t['name'])
        if re.search(r'\b(?:app|sim_tests_app)\(\)', t['body']) and 'c1020' not in hit:
            default_c1020.append(t['name'])
    emu_chr = collections.defaultdict(list)
    for t in all_tests:
        if t['emu'] and not t['ignored']:
            for c in set('c' + x for x in CHR_RE.findall(t['body'])):
                emu_chr[c].append(t['name'])

    kb_files = collections.defaultdict(set)
    live_lines = collections.Counter()
    for path, text in list(kb.items()) + [(p, t) for p, t in files.items() if p.endswith('.rs')]:
        for line in text.splitlines():
            cs = set('c' + x for x in CHR_RE.findall(line))
            if not cs:
                continue
            for c in cs:
                if path.startswith('docs/kb/') and not path.endswith(('coverage.md', 'log-archive.md', 'next.md')):
                    kb_files[c].add(os.path.basename(path))
                if 'rec_20' in line:
                    live_lines[c] += 1

    rows = []
    for c in sorted(seen):
        e = by_chr.get(c)
        placements = e['placements'] if e else []
        npc_rows = sorted({p['npc'] for p in placements})
        # name
        official = []
        for r in npc_rows:
            n = names.get(str(r))
            if n and n not in official:
                official.append(n)
        kind = None
        if bars is not None:
            ks = {bars.get(r) for r in npc_rows} - {None}
            kind = 'boss' if 'boss' in ks else ('mini-boss' if 'mini-boss' in ks else None)
        elif official:
            kind = 'bar'
        if official:
            name = '; '.join(official)
        elif c in KNOWN:
            name = KNOWN[c]
        elif e:
            name = '"%s" (param row name)' % e['name']
        else:
            name = '?'
        fights = [names.get(b, b) for b in casts.get(c, [])]
        # data present
        anim_chr = (e or {}).get('animChr', c)
        model = os.path.exists(os.path.join(EX, 'model_%s.bin' % c)) or bool(glob.glob(os.path.join(EX, 'model_%s_*.bin' % c)))
        anim = 'yes' if os.path.exists(os.path.join(EX, 'anim_%s.bin' % c)) else (
            'via ' + anim_chr if anim_chr != c and os.path.exists(os.path.join(EX, 'anim_%s.bin' % anim_chr)) else '-')
        thinks = sorted({p['think'] for p in placements if p.get('think', 0) >= 1000})
        bgoals = sorted({goals[t][0] for t in thinks if t in goals and goals[t][0] > 0})
        if not placements:
            ai = '-'
        elif not bgoals:
            ai = 'no battle goal'
        else:
            have = [g for g in bgoals if g in byte]
            ai = '%d/%d goals' % (len(have), len(bgoals))
            if len([g for g in bgoals if g in decomp]) < len(bgoals):
                ai += ' (src %d)' % len([g for g in bgoals if g in decomp])
        hks = 'yes' if os.path.exists(os.path.join(EX, 'hks_src', c + '.lua')) else (
            'via ' + anim_chr if anim_chr != c and os.path.exists(os.path.join(EX, 'hks_src', anim_chr + '.lua')) else '-')
        bnd = ''.join(sorted(bundles.get(c, set()), key='CABT'.index)) or '-'
        # loaded
        loaded = []
        generic = e is not None and os.path.exists(os.path.join(EX, 'enemies', c + '.json'))
        if generic:
            loaded.append('generic (enemies/%s.json)' % c)
        if named.get(c):
            loaded.append('named in ' + ', '.join(sorted(named[c])[:4]) + (' +%d' % (len(named[c]) - 4) if len(named[c]) > 4 else ''))
        # rules checked
        ts = sorted(set(test_chr.get(c, [])))
        rules = '%d: %s' % (len(ts), ', '.join(ts[:2]) + (', ...' if len(ts) > 2 else '')) if ts else '-'
        if c == 'c0000':
            rules = 'all %d sim tests drive Wolf' % len(sim)
        if c == 'c1020' and default_c1020:
            rules += ' (+%d more as the default enemy of app())' % len(default_c1020)
        # side by side
        sbs, compared = [], False
        if prof is not None and c in prof:
            sbs.append('AI profile vs %d recs (enemies_all.md)' % prof[c])
            compared = True
        if press is not None and c in press:
            sbs.append('reactions under pressure (enemies_pressure.md)')
            compared = True
        rp = set()
        for r in npc_rows:
            if r in unique_row:
                rp |= rp_rows.get(r, set())
        if rp:
            sbs.append('enemy replay csv in %d recs' % len(rp))
            compared = True
        if rp_press.get(c):
            sbs.append('pressure replay rows %s' % ','.join(str(x) for x in sorted(rp_press[c])))
            compared = True
        if c == 'c0000':
            ra = replay_all()
            if ra:
                g = sum(v[0] for v in ra.values())
                mt = sum(v[2] for v in ra.values())
                sbs.append('Wolf event replay: %d recs, %d/%d game events matched' % (len(ra), mt, g))
                compared = True
        tokens = [k for k, v in REF_FRAME_CHR.items() if v == c]
        shots = [f for f in ref if any(re.search(r'(^|_)%s(_|\.|$)' % k, f) for k in tokens)]
        if shots:
            sbs.append('%d ref_frames shots' % len(shots))
            compared = True
        # one-word names ("Headless") also sit inside other names ("Headless Ape"): ids and longer names only
        keys = [c] + [n.lower() for n in official if len(n.split()) > 1]
        rec_hits = [d for d, r in recs.items() if any(k in (r['name'] + ' ' + r['note']).lower() or k in d.lower() for k in keys)]
        if rec_hits:
            sbs.append('recorded live in %d' % len(rec_hits))
        if official and any(n.lower() in clips.lower() for n in official) or re.search(r'\b%s\b' % c, clips):
            sbs.append('reference clip')
        if live_lines[c]:
            sbs.append('%d kb/src lines cite a recording' % live_lines[c])
        if not recs and not os.path.isdir(REC):
            sbs.append('? (no recordings folder)')
        # notes
        notes = []
        if c in KNOWN_NOTES:
            notes.append(KNOWN_NOTES[c])
        if kind and e and 'dummy' in e['name'].lower():
            notes.append('the health bar sits on this dummy part')
        if fights:
            notes.append('in fight: ' + '; '.join(sorted(set(fights))))
        ar = sorted({arenas[str(r)] for r in npc_rows if str(r) in arenas})
        if ar:
            notes.append('arena ' + ', '.join(ar))
        if bgoals:
            notes.append('battle Lua ' + ','.join('%06d' % g for g in bgoals[:4]) + (' +%d' % (len(bgoals) - 4) if len(bgoals) > 4 else ''))
        hks_ref = sorted(p[4:] for p, t in files.items() if p.endswith('.rs') and not p.startswith('src/sim_tests/')
                         and re.search(r'\b%s(\.hks|\.lua|_transition|_cmsg|_define)' % c, t))
        if hks_ref:
            notes.append('its HKS cited in %d files (%s)' % (len(hks_ref), ', '.join(hks_ref[:3]) + (', ...' if len(hks_ref) > 3 else '')))
        if not e and 'enemies' not in seen[c]:
            only = sorted(seen[c])
            notes.append('not placed by any map (data: %s)' % '+'.join(only))
        if kb_files.get(c):
            notes.append('kb: ' + ', '.join(sorted(kb_files[c])))
        if emu_chr.get(c):
            notes.append('exe-checked: ' + ', '.join(sorted(set(emu_chr[c]))))
        # level
        if c == 'c0000':
            is_loaded = True
        else:
            is_loaded = generic or bool(named.get(c))
        level = 0
        if is_loaded:
            level = 1
        if ts or (c == 'c1020' and default_c1020) or (c == 'c0000'):
            level = 2 if is_loaded else level
        if emu_chr.get(c):
            level = 3
        if compared and is_loaded:
            level = 4
        rows.append({
            'chr': c, 'name': name, 'kind': kind or '', 'bnd': bnd, 'model': 'yes' if model else '-', 'anim': anim,
            'ai': ai, 'hks': hks, 'loaded': '; '.join(loaded) or '-', 'data': data_exe.get(c, '-'), 'rules': rules, 'sbs': '; '.join(sbs) or '-',
            'notes': '; '.join(notes) or '-', 'level': LEVELS[level], 'nrules': len(ts),
        })
    return rows, bars is not None


# ---------------------------------------------------------------- systems

# One row per system. files: globs under src/; kb: docs/kb notes; tests: regex on sim test names;
# emu: extracted/emu case-file prefixes; pilot: emulator-pilot.md "Pilot" ids; cats: recording
# categories; state: one line, hand-written from the kb notes (date in the note it comes from).
SYSTEMS = [
    {'name': 'Player movement', 'files': ['player.rs', 'actor.rs'], 'kb': ['movement.md', 'input-buffer.md'],
     'tests': r'jump|run|sprint|walk|step|crouch|fall|turn|locomotion|slide|stick|land|momentum|key_press',
     'emu': ['turn_frames', 'hks2', 'air'], 'pilot': ['7', 'hks2', 'air'], 'cats': {'movement'},
     'replay': ('events', r'Move|Run|Step|Jump|Sprint|Fall|Land|Kick|QuickTurn'),
     'state': 'HKS c0000 states hand-ported in player.rs; jump arc and fall control from the exe; turning from the stick checked on 440 emulator frames (2026-11-03); jump arc, walk-off, NoGravity, fall damage (wave 4).'},
    {'name': 'Combat rules (guard, deflect, damage, posture, hit stop)', 'files': ['combat.rs', 'actor.rs', 'status.rs'],
     'kb': ['guard-deflect.md', 'damage.md', 'attacks.md'],
     'tests': r'guard|deflect|posture|deathblow|hit_stop|hit|damage|perilous|mikiri|knock|break|super_arm|todome|poison|burn|death|reviv|resurrection|slash|attack|grab|throw',
     'emu': ['super_armor_hit', 'hit_stop', 'damage_', 'guard', 'modticks', 'throw', 'speffect', 'damagemod', 'hits', 'atkrec'], 'pilot': ['1', '2', '4', '6', 'guard', 'modticks', 'throw', 'speffect', 'speffect2', 'throw2', 'damagemod', 'hits', 'atkrec'], 'cats': {'combat'},
     'replay': ('events', r'Deflect|Guard|Attack'),
     'state': 'Params + decompile; super armour hit, the hit-stop table, HP and posture amounts, posture regen and the guard/deflect outcome checked in the emulator; hit-stop module, throw / deathblow range and states, SpEffect ticks (2026-11-03, wave 2); SpEffect stacking / regen / worn rows, throw sight / pick / plunge orbit, super armour refill, zero-amount hits, umbrella arc (wave 3); landed hits end to end (block chip, knockback start, guard-break push) and attack records (wave 4).'},
    {'name': 'Camera', 'files': ['camera.rs'], 'kb': ['camera.md'], 'tests': r'camera|lock|pitch',
     'emu': ['camera', 'lockcam', 'taecam', 'camwall', 'camera2'], 'pilot': ['3', '3b', 'camera2'], 'cats': {'camera'},
     'state': 'Follow camera checked on 1,626 emulator frames, walls and ground on 2,225 (2026-11-03); lock-on on 1,485, TAE camera events 151/153/155/156 on 2,160; 30 fps chase, pull-in, escape wall, lock at walls and the lock manipulator (wave 3).'},
    {'name': 'Enemy AI (C++ side)', 'files': ['enemy.rs', 'stealth.rs', 'duel.rs'], 'kb': ['enemy.md', 'frame-coverage.md'],
     'tests': r'enemy|enemies|patrol|respawn|ally|allys|msb|rival|line_up|lineup|group|drill',
     'emu': ['trace_enemy_frame', 'aicpp', 'aiframe', 'aimove'], 'pilot': ['aicpp', 'aiframe', 'aimove'], 'cats': {'enemies', 'bosses'},
     'state': 'Targeting and the native goals reimplemented; sight cones (52,554 placements) and the ai: distance queries checked against the exe (2026-11-03); targeting frames (9,414), nose sphere and sight ray (wave 3); goal-to-move, turn rates, search ring, enemy push (wave 4); interrupts and sound targets not checked.'},
    {'name': 'Enemy AI (Lua goals)', 'files': ['ai.rs', 'ai/lua50.rs'], 'kb': ['enemy.md'],
     'tests': r'real_ai|own_ai|think|brain|goal|kengeki|parry_timing|the_ai_',
     'emu': [], 'pilot': [], 'cats': {'enemies', 'bosses'},
     'state': "The game's own AI bytecode runs unchanged in a Lua 5.0 VM; engine API partly stubbed (ai_stubs)."},
    {'name': 'HKS', 'files': ['player.rs', 'enemy.rs'], 'kb': ['behaviour-graph.md'], 'tests': r'hks|_ref_\d',
     'emu': ['hks_env', 'hks2'], 'pilot': ['5', 'hks2'], 'cats': {'movement', 'combat'}, 'replay': ('recs', None),
     'state': 'No HKS runtime: c0000 rules hand-ported into player.rs, parts of c9997 into enemy.rs; 22 of 82 env ids checked against the exe on idle Wolf (hks_env.rs, 2026-11-03); 58 env rules (1,429 of 1,605 calls) and 29 acts checked in many poses (wave 3).'},
    {'name': 'TAE events', 'files': ['data.rs', 'anim.rs'], 'kb': ['attacks.md'], 'tests': r'tae',
     'emu': ['taewin', 'taecam', 'taestate'], 'pilot': ['taewin', 'taestate'], 'cats': set(),
     'state': "TAE events ported by id; the exe's event window (tae_window.rs) checked on 2,146 updates (2026-11-03); state events follow the exe's windows too (136,656 event-updates, wave 3)."},
    {'name': 'Behaviour graph', 'files': ['anim.rs', 'actor.rs'], 'kb': ['behaviour-graph.md'],
     'tests': r'cross_fade|blend|twist|clip|anim_set|pose',
     'emu': [], 'pilot': [], 'cats': set(),
     'state': 'Only loop flags and selector reuse read from c0000.hkx; state machines, layers and blends by hand.'},
    {'name': 'VFX (FXR)', 'files': ['fxr.rs', 'ffx.rs', 'vfx.rs', 'gore.rs', 'fx_*.wgsl'], 'kb': ['fxr.md'],
     'tests': r'effect|fxr|spfx|glow|stain|blood|burning_body|kanji|mark',
     'emu': ['fxr'], 'pilot': ['fxr', 'fxr2'], 'cats': {'effects'},
     'state': "TAE effects play the game's own FXR files through Sv1's FXR player; ffx.rs is the fallback for ids with no FXR. Curves, modifier random, shapes / spreads and particle frames (emission, motion, death) checked against the exe on 5 effects (2026-11-03)."},
    {'name': 'Sound', 'files': ['sound.rs', 'fmod.rs', 'area_music.rs'], 'kb': ['sound.md'],
     'tests': r'sound|muffle|music|heartbeat|hears|heard',
     'emu': [], 'pilot': [], 'cats': {'sound'},
     'state': "The game's FSB banks decoded and played from TAE / param lookups; area and boss music from the EMEVD tables."},
    {'name': 'Maps / collision', 'files': ['map.rs', 'fog_wall.rs', 'world.rs'], 'kb': ['map.md'],
     'tests': r'map|msb|region|ledge|wall|fog|metre_fall|proxy',
     'emu': ['proxy', 'collide'], 'pilot': ['proxy', 'collide'], 'cats': set(),
     'state': 'Boss arenas and a few map blocks exported with hit collision, fog gates and grapple edges; not the whole world. The player proxy at walls and on open ground checked against the exe (2026-11-03); body push and floor follow too (wave 3); step-up derived, rim leave point a gap.'},
    {'name': 'Rendering / shaders / lighting', 'files': ['model.rs', 'cloth.rs', 'sky.rs', 'height_fog.rs', 'grading.rs', 'sekiro_material.wgsl', 'map_material.wgsl', 'sky.wgsl', 'height_fog.wgsl', 'grading_lut.wgsl'],
     'kb': ['visuals.md', 'sky_shader.md'], 'tests': r'model|cloth|draw_mask|render|visible|coat',
     'emu': [], 'pilot': [], 'cats': set(),
     'state': "Game shaders decoded statically and ported per material family; cloth solver read from the exe; still shots against real frames."},
    {'name': 'Event scripts (EMEVD)', 'files': ['emevd.rs', 'enemy/script.rs', 'enemy/events.rs'], 'kb': [],
     'tests': r'phase|defeat|hands_over|becomes|follows_emma|fight_reset|reset|scripted|bar',
     'emu': [], 'pilot': [], 'cats': {'script'},
     'state': 'Boss EMEVD events run by an interpreter (enemy/script.rs); the world map scripts are not run.'},
    {'name': 'Menus / HUD / saves', 'files': ['menu.rs', 'hud.rs', 'debug_menu.rs', 'icon.rs', 'icon_dot.rs'], 'kb': [],
     'tests': r'menu|hud|title|picker|health_bar|bar_names|cheat',
     'emu': [], 'pilot': [], 'cats': set(),
     'state': 'Title / pause / death / won screens, posture and HP bars, debug menu; no game menus, no saves.'},
    {'name': 'Cutscenes', 'files': [], 'kb': [], 'tests': r'cutscene', 'emu': [], 'pilot': [], 'cats': set(),
     'state': 'Not ported.'},
    {'name': 'NPC quests / dialogue', 'files': ['enemy/talk.rs'], 'kb': [], 'tests': r'talk', 'emu': [], 'pilot': [], 'cats': set(),
     'state': 'One pre-fight talk (Owl Father, t100121); no quests, no other dialogue.'},
    {'name': 'Prosthetics', 'files': ['prosthetic.rs', 'model/tool.rs', 'enemy_bullet.rs'], 'kb': ['skills.md'],
     'tests': r'tool|prosthetic|shuriken|firecracker|flame_vent|axe|umbrella|sabimaru|mist_raven|spear|whistle|okinaga|sen_throw|emblem|bullet|oil_pot',
     'emu': ['items'], 'pilot': ['items'], 'cats': {'skills', 'items'}, 'replay': ('recs', {'skills'}),
     'state': 'Every tool starts and costs its emblems; bullets from the Bullet params. Gourd use and the Loaded Axe payment checked against the exe (2026-11-03); goods bullets (pellet, sugars) not ported.'},
    {'name': 'Skills', 'files': ['status.rs', 'prosthetic.rs'], 'kb': ['skills.md'],
     'tests': r'art|skill|ichimonji|mortal_draw|whirlwind|floating_passage|shadowrush|sakura|dragon_flash|nightjar|latent|flowing_water',
     'emu': [], 'pilot': [], 'cats': {'skills'}, 'replay': ('recs', {'skills'}),
     'state': 'Combat arts and latent skills ported; arts cost emblems and unlock their moves.'},
    {'name': 'Grapple', 'files': ['wire.rs', 'enemy/grapple.rs'], 'kb': ['grapple.md'], 'tests': r'grapple|wire|hook',
     'emu': [], 'pilot': [], 'cats': {'movement'},
     'state': 'Points, shoot, reel, launch and landing ported (wire.rs, map.rs grapple_target).'},
    {'name': 'Stealth', 'files': ['stealth.rs'], 'kb': ['enemy.md'],
     'tests': r'stealth|unaware|crouching_cuts|heard|sight|cautious|feigning|covert|deafened|notices',
     'emu': ['aicpp', 'aiframe'], 'pilot': ['aicpp', 'aiframe'], 'cats': {'enemies'},
     'state': "Perception and target states ported from the exe's SprjTargetingSystem (stealth.rs)."},
    {'name': 'Docking (wall hug, ledge hang)', 'files': ['docking.rs'], 'kb': ['docking.md'], 'tests': r'hang_|hug_|dock',
     'emu': [], 'pilot': [], 'cats': set(),
     'state': 'docking.rs exists; kb/docking.md (2026-10-11) calls it parked, not built.'},
]


def pilot_rows():
    t = read(os.path.join(REPO, 'docs', 'kb', 'emulator-pilot.md')) or ''
    out = {}
    for line in t.splitlines():
        c = [x.strip() for x in line.split('|')]
        if len(c) > 4 and re.fullmatch(r'\d+[a-z]?', c[1]):
            out[c[1]] = c[2]
    return out


def systems(files, all_tests, kb):
    pilots = pilot_rows()
    emu_dir = os.path.join(EX, 'emu')
    emu_cases = sorted(os.path.basename(f)[:-5] for f in glob.glob(os.path.join(emu_dir, '*.json')))
    recs = recordings()
    ra = replay_all()
    rev = replay_events()
    prof = enemies_all()
    press = enemies_pressure()
    ref = os.listdir(os.path.join(EX, 'ref_frames')) if os.path.isdir(os.path.join(EX, 'ref_frames')) else []
    out = []
    for s in SYSTEMS:
        paths = []
        for g in s['files']:
            paths += sorted(rel(p) for p in glob.glob(os.path.join(REPO, 'src', g)))
        fl = ', '.join('%s (%d)' % (p[4:], (files.get(p) or '').count('\n')) for p in paths) or '-'
        kbs = [k for k in s['kb'] if os.path.exists(os.path.join(REPO, 'docs', 'kb', k))]
        kbcell = ', '.join(kbs) or 'none'
        mine = [t for t in all_tests if not t['ignored']]
        emu_tests = sorted({t['name'] for t in mine if t['emu'] and (t['file'] in paths or (s['tests'] and re.search(s['tests'], t['name'])))})
        # an emu test belongs to the systems its case file belongs to
        emu_tests = [n for n in emu_tests if any(p in next(t['body'] for t in mine if t['name'] == n) for p in s['emu'])] if s['emu'] else []
        piece = []
        if emu_tests:
            piece.append(', '.join(emu_tests))
        if s['pilot']:
            piece.append('pilot ' + ', '.join(p for p in s['pilot'] if p in pilots))
        cases = [c for c in emu_cases if any(c.startswith(p) for p in s['emu'])]
        untested = [c for c in cases if not any(c in t['body'] for t in mine if t['emu'])]
        if untested:
            piece.append('cases without a test yet: ' + ', '.join(untested))
        emucell = '; '.join(piece) or '-'
        unit = [t for t in mine if not t['sim'] and not t['emu'] and t['file'] in paths]
        word = r'(?:^|_)(?:%s)' % s['tests']  # a keyword starts a word of the test name
        simt = [t for t in mine if t['sim'] and not t['emu'] and re.search(word, t['name'])]
        other = '%d (%d unit, %d sim)' % (len(unit) + len(simt), len(unit), len(simt))
        sbs, compared = [], False
        kind, arg = s.get('replay') or (None, None)
        if kind == 'events' and rev:
            hit = {k: v for k, v in rev.items() if re.search(arg, k)}
            if hit:
                sbs.append('Wolf replay vs the game, %d listed events (%s): %d/%d matched' % (
                    len(hit), ', '.join(sorted(hit)[:3]) + (', ...' if len(hit) > 3 else ''),
                    sum(v[2] for v in hit.values()), sum(v[0] for v in hit.values())))
                compared = True
        if kind == 'recs' and ra:
            hit = {r: v for r, v in ra.items() if arg is None or recs.get(r, {}).get('cats', set()) & arg}
            if hit:
                sbs.append('Wolf replay vs the game, all HKS events of %d recs%s: %d/%d matched' % (
                    len(hit), '' if arg is None else ' with category ' + '/'.join(sorted(arg)),
                    sum(v[2] for v in hit.values()), sum(v[0] for v in hit.values())))
                compared = True
        if s['name'] == 'Enemy AI (Lua goals)' and prof:
            sbs.append('attack picks by distance vs the game for %d kinds (enemies_all.md)' % len(prof))
            compared = True
        if s['name'] == 'Enemy AI (C++ side)' and press:
            sbs.append('reactions under pressure for %d kind (enemies_pressure.md)' % len(press))
            compared = True
        if s['name'].startswith('Rendering'):
            n = len([f for f in ref if f.startswith(('real_', 'coat_', 'detail_'))])
            if n:
                sbs.append('%d ref_frames stills (real vs Sv1, visuals.md)' % n)
                compared = True
        if s['cats']:
            nrec = len([r for r in recs.values() if r['cats'] & s['cats']])
            if nrec:
                sbs.append('%d recordings with category %s' % (nrec, '/'.join(sorted(s['cats']))))
        if not os.path.isdir(REC):
            sbs.append('? (no recordings folder)')
        level = 0
        if paths:
            level = 1
        if paths and (unit or simt):
            level = 2
        if emu_tests or (s['pilot'] and any(p in pilots for p in s['pilot'])):
            level = 3
        if compared:
            level = 4
        out.append({'name': s['name'], 'files': fl, 'kb': kbcell, 'emu': emucell, 'other': other,
                    'sbs': '; '.join(sbs) or '-', 'state': s['state'], 'level': LEVELS[level]})
    return out


# ---------------------------------------------------------------- output

DEFAULT_INTRO = """# Coverage: how far Sv1 is proven to match Sekiro

(Intro: replace this paragraph by hand; tools/coverage.py keeps it.)
"""


def main():
    t0 = time.time()
    if not os.path.isdir(EX):
        sys.exit('no extracted data at %s (set SHINOBI_ROOT)' % EX)
    files = src_files()
    all_tests = tests(files)
    kb = {rel(p): read(p) for p in glob.glob(os.path.join(REPO, 'docs', 'kb', '*.md'))}
    chars, bars_ok = characters(files, all_tests, kb)
    syss = systems(files, all_tests, kb)

    old = read(OUT) or ''
    intro = old.split(MARKER)[0].rstrip('\n') + '\n' if MARKER in old else DEFAULT_INTRO
    lv = collections.Counter(c['level'] for c in chars)
    kinds = collections.Counter(c['kind'] for c in chars if c['kind'])
    L = [intro, MARKER, '']
    L.append('Generated %s from %s and %s.' % (time.strftime('%Y-%m-%d'), EX.replace('\\', '/'),
                                             REC.replace('\\', '/') if os.path.isdir(REC) else 'no recordings folder'))
    L.append('')
    L.append('## Summary')
    L.append('')
    L.append('- Characters: %d chr ids (%d placed by the maps), %d bosses, %d mini-bosses%s.' % (
        len(chars), len([c for c in chars if c['loaded'].startswith('generic')]), kinds.get('boss', 0), kinds.get('mini-boss', 0),
        '' if bars_ok else ' (EMEVD read failed: bar rows marked "bar")'))
    L.append('- Characters by proof level: ' + ', '.join('%s %d' % (k, lv.get(k, 0)) for k in LEVELS) + '.')
    sl = collections.Counter(s['level'] for s in syss)
    L.append('- Systems by proof level: ' + ', '.join('%s %d' % (k, sl.get(k, 0)) for k in LEVELS) + '.')
    L.append('')
    L.append('## Characters')
    L.append('')
    L.append('| chr | Name | Kind | Bundles | Model | Anim | AI Lua | HKS | Loaded by Sv1 | Data vs exe | Rules checked (sim tests) | Side by side | Notes | Proof level |')
    L.append('|---|---|---|---|---|---|---|---|---|---|---|---|---|---|')
    for c in chars:
        L.append('| ' + ' | '.join(cell(c[k]) for k in ('chr', 'name', 'kind', 'bnd', 'model', 'anim', 'ai', 'hks', 'loaded', 'data', 'rules', 'sbs', 'notes', 'level')) + ' |')
    L.append('')
    L.append('## Systems')
    L.append('')
    L.append('| System | Sv1 files (lines) | kb note | Emulator-checked | Other tests | Side by side | State | Proof level |')
    L.append('|---|---|---|---|---|---|---|---|')
    for s in syss:
        L.append('| ' + ' | '.join(cell(s[k]) for k in ('name', 'files', 'kb', 'emu', 'other', 'sbs', 'state', 'level')) + ' |')
    L.append('')
    with open(OUT, 'w', encoding='utf-8', newline='\n') as f:
        f.write('\n'.join(L))
    print('%d characters, %d systems -> %s (%.1f s)' % (len(chars), len(syss), rel(OUT), time.time() - t0))
    print('characters:', dict(lv))
    print('kinds:', dict(kinds))
    print('systems:', dict(sl))


if __name__ == '__main__':
    main()
