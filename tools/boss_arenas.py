"""Each boss's own arena: the map around its MSB placement (docs/kb/map.md).

Reads extracted/enemies/boss_scripts.json (tools/boss_scripts.py: every boss script's map and
boss entity id) and, one map at a time, unpacks the map's pieces, hit collision, envmaps, area
textures and draw params from your own Sekiro install, exports the arena around the boss with
`sekiro-extract map` (MAP_NAME=boss_<NpcParam row>, centre = the boss's entity id) and deletes
the map's raw unpacked files again (about 2 GB each; the extractor can recreate them).

A boss whose entity stands in another boss's cast within SHARE_M of it fights in that boss's
arena (Lady Butterfly's second phase, the Divine Dragon's fight). Writes
extracted/map_boss_arenas.json: {NpcParam row: map stem}, read by src/map.rs boss_arena.

Usage: python tools/boss_arenas.py [-Sekiro <install>] [-force] [map id ...]   (no ids: all of
them; -force re-exports arenas that already exist, after an exporter change)
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
OVERRIDES = {51000100: {'MAP_CENTRE_YAW': '180'}, 52000000: {'MAP_CENTRE_DIST': '24'}, 53000000: {'MAP_CENTRE_DIST': '24'}}
# The user's rule (2026-10-11): only fights whose mechanics live in their surroundings get an
# arena of their own; every other boss fights on Isshin the Sword Saint's flower field.
OWN_ROWS = {
    50000000, 50001000,  # Corrupted Monk: the Mibu phantoms' spots, the Fountainhead bridge
    50900000, 50900001,  # Lady Butterfly: the illusions' spots, the burning hall
    51000000, 51000100,  # Guardian Ape, Headless Ape and the brown ape: the valley's pond and grotto
    52000000, 53000000,  # Divine Dragon and the Old Dragons: the cloud sea and the lightning trees
}
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
    force = '-force' in argv
    only = set(argv) - {'-force'}
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

    by_map = {}
    for row, (m, ent) in own.items():
        by_map.setdefault(m, []).append((row, ent))
    env = dict(os.environ, SEKIRO_DIR=sekiro, MAP_LOD=LOD)
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
        run([EXE, 'bxf', hit, os.path.join(OUT, 'map', m, 'hit')], env=env, quiet=True)
        for row, ent in todo:
            args = [EXE, 'map', OUT, m, str(ent), RADIUS]
            env_row = dict(env, MAP_NAME=f'boss_{row}', **OVERRIDES.get(int(row), {}))
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
