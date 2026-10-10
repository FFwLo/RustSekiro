"""Game effects for the FXR player (src/fxr.rs), from the user's own Sekiro data:

  python tools/fxr_extract.py

1. Every FFX id the exported characters use (TAE FFXID args, bullet sfxId_*, the blood FFX) ->
   extracted/fxr/<id>.json, converted from sfx/sfxbnd_commoneffects.ffxbnd's f<id>.fxr by
   tools/fxr-dump/dump.mjs (@cccode/fxr, public domain: github.com/EvenTorset/fxr), following
   ReferenceNode (2001 "sfx") chains.
2. Every texture those effects name (texture / mask / normalMap / layerN ids) -> extracted/fxr_tex/
   <id>.png from the archive's s<id:05>.tpf (one DDS each), via sekiro-extract dds2png.

Needs: `sekiro-extract unpack <Sekiro dir> extracted 'sfxbnd_commoneffects'`, combat_data.json,
`npm i` in tools/fxr-dump, and the built extractor (tools/sekiro-extract).
"""
import glob
import json
import os
import re
import struct
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SFX = os.path.join(ROOT, 'extracted/sfx/sfxbnd_commoneffects.ffxbnd.d')
OUT = os.path.join(ROOT, 'extracted/fxr')
TEX = os.path.join(ROOT, 'extracted/fxr_tex')
# SEKIRO_EXTRACT (tools/extract.ps1 passes the one it used), else a source build: release, then debug.
EXTRACT = os.environ.get('SEKIRO_EXTRACT') or next(
    (p for p in (os.path.join(ROOT, f'tools/sekiro-extract/target/{b}/sekiro-extract.exe') for b in ('release', 'debug')) if os.path.exists(p)),
    os.path.join(ROOT, 'tools/sekiro-extract/target/release/sekiro-extract.exe'))
# The blood sprays gore.rs drives from the TAE.
EXTRA = [220502, 220503, 220505, 220506]


def used_ids():
    d = json.load(open(os.path.join(ROOT, 'extracted/combat_data.json'), encoding='utf-8'))
    ids = set(EXTRA)
    for c in [d['player']] + list(d['enemies'].values()):
        for a in c['anims'].values():
            for e in a.get('events', []):
                args = e.get('args')
                if isinstance(args, dict) and isinstance(args.get('FFXID'), int) and args['FFXID'] > 0:
                    ids.add(args['FFXID'])
        for coll in ('bulletRows', 'bullets'):
            for b in (c.get(coll) or {}).values():
                if isinstance(b, dict):
                    for k in ('sfxId_Bullet', 'sfxId_Hit', 'sfxId_Flick', 'sfxId_ForceErase'):
                        if isinstance(b.get(k), int) and b[k] > 0:
                            ids.add(b[k])
    # Status effects on a character (burning, poison): SpEffect vfxId / vfxId1 -> SpEffectVfxParam
    # initSfxId (once) / midstSfxId (while it lasts), src/status.rs.
    vfx = json.load(open(os.path.join(ROOT, 'extracted/json/params/SpEffectVfxParam.json'), encoding='utf-8'))['rows']
    vfx = {r['id']: r for r in (vfx.values() if isinstance(vfx, dict) else vfx)}
    for s in d['player'].get('spEffects', {}).values():
        for k in ('vfxId', 'vfxId1'):
            r = vfx.get(s.get(k, -1))
            for f in ('initSfxId', 'midstSfxId'):
                if r and r.get(f, -1) > 0:
                    ids.add(r[f])
    # Hit / guard / deflect sparks: every FXR id in HitEffectSfxParam (vfx.rs, combat.rs).
    for r in d['params'].get('HitEffectSfxParam', {}).values():
        for k, v in r.items():
            if k != 'id' and isinstance(v, int) and v > 0:
                ids.add(v)
    return ids


def dump(ids):
    ids = [i for i in sorted(ids) if os.path.exists(os.path.join(SFX, f'f{i:09d}.fxr')) and not os.path.exists(os.path.join(OUT, f'{i}.json'))]
    for k in range(0, len(ids), 60):
        subprocess.run(['node', os.path.join(ROOT, 'tools/fxr-dump/dump.mjs'), SFX, OUT] + [str(i) for i in ids[k:k + 60]], check=True, stdout=subprocess.DEVNULL)


def main():
    os.makedirs(OUT, exist_ok=True)
    os.makedirs(TEX, exist_ok=True)
    dump(used_ids())
    # References (2001 "sfx": id) until nothing new.
    while True:
        have = {int(os.path.basename(f)[:-5]) for f in glob.glob(os.path.join(OUT, '*.json'))}
        refs = set()
        for f in glob.glob(os.path.join(OUT, '*.json')):
            refs.update(int(x) for x in re.findall(r'"sfx": (\d+)', open(f).read()))
        new = {r for r in refs - have if os.path.exists(os.path.join(SFX, f'f{r:09d}.fxr'))}
        if not new:
            break
        dump(new)
    tex = set()
    for f in glob.glob(os.path.join(OUT, '*.json')):
        for k, v in re.findall(r'"(texture|mask|normalMap|layer1|layer2|layer3)": (\d+)', open(f).read()):
            if int(v) > 0:
                tex.add(int(v))
    done = 0
    for t in sorted(tex):
        png = os.path.join(TEX, f'{t}.png')
        tpf = os.path.join(SFX, f's{t:05d}.tpf')
        if os.path.exists(png) or not os.path.exists(tpf):
            continue
        d = open(tpf, 'rb').read()
        doff, dsz = struct.unpack_from('<Ii', d, 0x10)
        dds = os.path.join(TEX, f'{t}.dds')
        open(dds, 'wb').write(d[doff:doff + dsz])
        if subprocess.run([EXTRACT, 'dds2png', dds, png], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL).returncode == 0:
            done += 1
        os.remove(dds)
    print(f'{len(glob.glob(os.path.join(OUT, "*.json")))} effects, {len(tex)} textures ({done} new)')


if __name__ == '__main__':
    sys.exit(main())
