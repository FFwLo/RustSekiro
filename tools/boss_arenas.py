"""Each boss's own arena: the map around its MSB placement (docs/kb/map.md).

Reads extracted/enemies/boss_scripts.json (tools/boss_scripts.py: every boss script's map and
boss entity id) and, one map at a time, unpacks the map's pieces, hit collision, envmaps, area
textures and draw params from your own Sekiro install, exports the arena around the boss with
`sekiro-extract map` (MAP_NAME=boss_<NpcParam row>, centre = the boss's entity id) and deletes
the map's raw unpacked files again (about 2 GB each; the extractor can recreate them).

A boss whose entity stands in another boss's cast within SHARE_M of it fights in that boss's
arena (Lady Butterfly's second phase, the Divine Dragon's fight). Writes
extracted/map_boss_arenas.json: {NpcParam row: map stem}, read by src/map.rs boss_arena.

Usage: python tools/boss_arenas.py [-Sekiro <install>] [-force] [-edges] [map id ...]   (no ids:
all of them; -force re-exports arenas that already exist, after an exporter change; -edges
re-exports only each arena json's grapple points and ledges, exporter MAP_EDGES_ONLY: pieces,
collision, gates, probes, LUTs and draw params stay as they are)
"""
import json
import os
import re
import shutil
import subprocess
import sys

# The extractor prints Japanese material names: never let a cp1252 console kill the run.
for _stream in (sys.stdout, sys.stderr):
    if hasattr(_stream, 'reconfigure'):
        _stream.reconfigure(errors='replace')

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
OUT = os.path.join(ROOT, 'extracted')
EXE = os.environ.get('SEKIRO_EXTRACT') or next(
    (p for p in (os.path.join(ROOT, 'tools', 'sekiro-extract', 'sekiro-extract.exe'),
                 os.path.join(ROOT, 'tools', 'sekiro-extract', 'target', 'release', 'sekiro-extract.exe')) if os.path.exists(p)),
    os.path.join(ROOT, 'tools', 'sekiro-extract', 'target', 'release', 'sekiro-extract.exe'))
RADIUS = '60'
LOD = '20,40'
# Maps with an open horizon get the distant-view and sky pieces (exporter MAP_FAR, metres):
# the Divine Dragon's cloud sea shows the mountains and forests round it (kb/map.md "Far view").
FAR = {'m25_00_00_00': '3000'}
SHARE_M = 40.0
# Per-row exporter settings (sekiro-extract map env): the Headless Ape's MSB pose faces the
# pit's wall 3 m off while his fight region (1702870) runs 43 m the other way, so his arena
# is turned round and Wolf starts down the pit.
# The Divine Dragon's MSB spot sits in a cluster of rock and tree pieces: the fight stands
# 24 m ahead of it on the cloud floor (h003100), the Old Dragons' row the same.
# The castle top (Genichiro and the fights moved onto it): its burnt floor m015072 overlaps the tatami
# m015070 these fights show (sekiro-rs `hidden_models`), so the exporter leaves it out. Its
# draw param variants m11_01_0740 / _0750 are the ones the castle's events switch to for the
# Emma / Isshin Ashina and the Owl fights (gparam sub 40 / 50; src/map.rs FIGHT_LIGHTING).
# The Folding Screen Monkeys' bar sits on a c1000 dummy (2000800) at one end of the temple
# hall; the four monkeys and the horde spread up to 118 m from it (boss_scripts.json 12600050),
# so the pieces are picked round the second monkey (2000802, mid-hall) out to 85 m.
# Gyoubu's MSB spot (1100800) faces the castle gate's walls and steps: his arena is turned round
# and stands 16 m into the open field behind him, where his horse charges run.
# ('RADIUS' is the exporter's radius argument, the rest its env.)
# The dragons (the user, 2026-10-15: "devine dragon need bigger arena becauce he push you
# away"): the strong wind 52000830 blows Wolf across the cloud floor and the Old Dragons' fall
# (m25 12505885 WarpCopy) hands over to the Divine Dragon 64 m from their own spot, so both
# arenas reach 140 m: each holds the other's place.
OVERRIDES = {51000100: {'MAP_CENTRE_YAW': '180'}, 50800000: {'MAP_CENTRE_YAW': '180', 'MAP_CENTRE_DIST': '16'}, 52000000: {'MAP_CENTRE_DIST': '24', 'RADIUS': '140'}, 53000000: {'MAP_CENTRE_DIST': '24', 'RADIUS': '140'},
             71001000: {'MAP_SKIP_MODEL': 'm015072', 'MAP_GPARAM_EXTRA': '0740,0750'},
             12600050: {'MAP_PICK': '2000802', 'RADIUS': '85'}}
# The user's rule (2026-10-11): only fights whose mechanics live in their surroundings get an
# arena of their own; every other boss fights on Isshin the Sword Saint's flower field.
OWN_ROWS = {
    50000000, 50001000,  # Corrupted Monk: the Mibu phantoms' spots, the Fountainhead bridge
    50900000, 50900001,  # Lady Butterfly: the illusions' spots, the burning hall
    51000000, 51000100,  # Guardian Ape, Headless Ape and the brown ape: the valley's pond and grotto
    52000000, 53000000,  # Divine Dragon and the Old Dragons: the cloud sea and the lightning trees
    # The user's call (2026-10-11, "use his arena"): Genichiro on the Ashina Castle top, as
    # AKJama/sekiro-rs crates/sim/src/arena.rs ARENAS (c7100_0000, m11_01_00_00, 60 m). Way of
    # Tomoe (71100000) shares it.
    71001000, 71100000,
    # The Folding Screen Monkeys (2026-10-12, "work on the rest of the bosses"): the fight is
    # the temple hall itself, its rooms, beams and the regions the monkeys hide and call in.
    12600050,
    # Every main boss in its own place (2026-10-17): the Sword Saint's silvergrass field holds
    # the prologue Genichiro (71000000) and the Way of Tomoe hand-over (71101000) too, the Owl
    # (Father) fights in Hirata (m10), Gyoubu and the Demon of Hatred in the Ashina Outskirts (m11_00).
    54000000, 71000000, 71101000, 50601010, 50800000, 70200000,
}
# Fights moved onto another boss's arena (the user's call, 2026-10-11: "put Isshin from the
# flower arena to the top", "isshin other form", "put emma at the top"): the Sword Saint fight,
# opened by Genichiro, Way of Tomoe (71101000), and the Shura ending's Emma, the Gentle Blade
# (74000010) and Isshin Ashina (54300000), both fought on the castle top in the game too, are on
# Genichiro's castle top. The silvergrass field stays exported as 54000000's map for the
# bosses that share it. The Great Shinobi Owl (50600000) too: his own spot c5060_0000 stands at
# the platform's edge, so the start 8 m behind him fell outside the railing onto the roof tiles
# 2.7 m below; sekiro-rs starts both fights at the same player point (c0000_0003).
# Since 2026-10-17 only the fights the game holds on the castle top itself stay moved (Emma and
# Isshin Ashina, the Owl: their own spots are on m11_01 too, the Owl's start fixed by the move).
MOVED = {54300000: 71001000, 74000010: 71001000, 50600000: 71001000}
# The user's rule (2026-10-12): "all the bosses the im working on should be in the top of castle
# unless they dont fit". Every boss / mini-boss row of extracted/enemies/npc_names.json fights on
# the castle top (boss_71001000), except the own-arena fights above (their mechanics live in
# their surroundings) and these two, too big for the roof: Gyoubu's horseback charges across a
# field and the Demon of Hatred (both stay on Isshin's silvergrass field, SHARED_ROW). The two
# underwater Headless (c1340 13400000 / 13401000) come too: Sv1 has no swimming.
# Added to the index only (no export); the mini-boss rows have no boss script.
DONT_FIT = {50800000, 70200000}
TOP_ROW = 71001000
SHARED_ROW = 54000000  # Isshin, the Sword Saint: the silvergrass field below the castle


def run(args, env=None, quiet=False, capture=False):
    print('>', ' '.join(args), flush=True)
    r = subprocess.run(args, env=env, capture_output=quiet or capture, text=True, encoding='utf-8', errors='replace')
    if quiet:
        print(*r.stdout.splitlines()[-2:], r.stderr[-2000:], sep='\n', flush=True)
    elif capture:
        print(r.stdout, r.stderr, flush=True)
    r.check_returncode()
    return r.stdout


def main():
    argv = sys.argv[1:]
    sekiro = r'C:\Program Files (x86)\Steam\steamapps\common\Sekiro'
    if argv[:1] == ['-Sekiro']:
        sekiro, argv = argv[1], argv[2:]
    # `-force`: re-export the arenas that already exist (a changed exporter).
    edges = '-edges' in argv
    force = '-force' in argv or edges
    only = set(argv) - {'-force', '-edges'}
    scripts = json.load(open(os.path.join(OUT, 'enemies', 'boss_scripts.json'), encoding='utf-8'))
    index_path = os.path.join(OUT, 'map_boss_arenas.json')
    index = json.load(open(index_path, encoding='utf-8')) if os.path.exists(index_path) else {}

    # Which rows get their own export, and which share one.
    own = {}  # row -> (map, entity)
    for row, s in sorted(scripts.items()):
        if int(row) not in OWN_ROWS and int(row) != SHARED_ROW:
            index[row] = f'boss_{SHARED_ROW}'
            continue
        host = None
        for other, o in sorted(scripts.items()):
            if other == row or o['map'] != s['map']:
                continue
            c = o['chars'].get(str(s['boss']))
            if c and sum(x * x for x in c['at']) ** 0.5 <= SHARE_M and (other in own or other < row):
                host = other
                break
        if host is not None and host in own:
            index[row] = f'boss_{host}'
            print(f'{row}: shares boss_{host}')
        else:
            own[row] = (s['map'], s['boss'])
            index[row] = f'boss_{row}'
    for row, host in MOVED.items():
        index[str(row)] = f'boss_{host}'
    names = json.load(open(os.path.join(OUT, 'enemies', 'npc_names.json'), encoding='utf-8'))['rows']
    for row in names:
        if int(row) in OWN_ROWS or int(row) in DONT_FIT or int(row) == TOP_ROW:
            continue
        if int(row) in MOVED or row not in index or index[row] == f'boss_{SHARED_ROW}':
            index[row] = f'boss_{TOP_ROW}'
    # The moved rows, for enemy.rs own_arena_of: a stranger on its arena even when the arena is
    # on its own map (the Owl on the castle top), so its script's place warps are skipped.
    index['moved'] = sorted(MOVED)

    by_map = {}
    for row, (m, ent) in own.items():
        by_map.setdefault(m, []).append((row, ent))
    env = dict(os.environ, SEKIRO_DIR=sekiro, MAP_LOD=LOD)
    if edges:
        env['MAP_EDGES_ONLY'] = '1'
    for m, rows in sorted(by_map.items()):
        if only and m not in only:
            continue
        env['MAP_FAR'] = os.environ.get('MAP_FAR', FAR.get(m, ''))
        todo = [(r, e) for r, e in rows if force or not os.path.exists(os.path.join(OUT, f'map_boss_{r}.bin'))]
        extra_areas = set()
        if not todo:
            continue
        area, block = m[:3], m[:6]
        if not os.path.isdir(os.path.join(OUT, 'map', m)):
            run([EXE, 'unpack', sekiro, OUT, f'map/{m}/|map/{m}_envmap|map/{area}/|param/drawparam/{block}|other/maptex'], env=env, quiet=True)
        hit = os.path.join(OUT, 'map', m, f'h{m[1:]}.hkxbhd')
        if not edges:
            run([EXE, 'bxf', hit, os.path.join(OUT, 'map', m, 'hit')], env=env, quiet=True)
        for row, ent in todo:
            over = dict(OVERRIDES.get(int(row), {}))
            args = [EXE, 'map', OUT, m, str(ent), over.pop('RADIUS', RADIUS)]
            env_row = dict(env, MAP_NAME=f'boss_{row}', **over)
            out = run(args, env=env_row, capture=True)
            # The objects there (obj/<model>.objbnd) and the texture packs of other areas (the
            # Headless Ape's ground uses m13's m13_ground_900), unpacked on the first pass, then
            # again.
            need = [l.split(': ', 1)[1] for l in out.splitlines() if l.startswith('objects to unpack: ')]
            for l in out.splitlines():
                if l.startswith('textures: '):
                    for a in sorted(set(re.findall(r'"(m\d\d)_', l.split('missing', 1)[-1]))):
                        if a != area and not os.path.isdir(os.path.join(OUT, 'map', a)):
                            need.append(f'map/{a}/')
                            extra_areas.add(a)
            if need:
                run([EXE, 'unpack', sekiro, OUT, '|'.join(need)], env=env, quiet=True)
                run(args, env=env_row, capture=True)
        # The raw map (kept for the General's own m11_01_00_00 arena, extract.ps1).
        if m != 'm11_01_00_00':
            for d in (os.path.join(OUT, 'map', m), os.path.join(OUT, 'map', f'{m}_envmap')):
                shutil.rmtree(d, ignore_errors=True)
        if area != 'm11' and not any(k[:3] == area and k > m for k in by_map):
            shutil.rmtree(os.path.join(OUT, 'map', area), ignore_errors=True)
        for a in extra_areas:
            if a != 'm11' and not any(k[:3] == a and k > m for k in by_map):
                shutil.rmtree(os.path.join(OUT, 'map', a), ignore_errors=True)
        json.dump(index, open(index_path, 'w', encoding='utf-8', newline='\n'), indent=1, sort_keys=True)
    json.dump(index, open(index_path, 'w', encoding='utf-8', newline='\n'), indent=1, sort_keys=True)
    print(json.dumps(index, indent=1, sort_keys=True))


if __name__ == '__main__':
    main()
