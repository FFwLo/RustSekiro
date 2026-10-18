"""Game effects for the FXR player (src/fxr.rs), from the user's own Sekiro data:

  python tools/fxr_extract.py

1. Every FFX id the exported characters use (TAE FFXID args of Wolf and every exported enemy in
   extracted/enemies, bullet sfxId_*, the blood FFX) -> extracted/fxr/<id>.json, converted from
   f<id>.fxr of sfx/sfxbnd_commoneffects.ffxbnd, else of the area archives sfxbnd_m10-m25 (the
   characters' own effects, 6xxxxx: the Ape's 651000, the Monk's 6535xx ...) by
   tools/fxr-dump/dump.mjs (@cccode/fxr, public domain: github.com/EvenTorset/fxr), following
   ReferenceNode (2001 "sfx") chains.
2. Every texture those effects name (texture / mask / normalMap / layerN ids) -> extracted/fxr_tex/
   <id>.png from the archive's s<id:05>.tpf (one DDS each), via sekiro-extract dds2png.
3. Every mesh a Model appearance (605) names -> extracted/fxr_model/model_<id>.bin via
   `sekiro-extract model s<id:05>.flver <diffuse>.tpf`: the diffuse is the flver's first "_a"
   texture, else its own s<id:05>, else its first texture that is not _n / _r / _m (this picks
   every pair of the old hand list in tools/extract.ps1: 4010 -> s04009_a, 8060 -> s34030 ...).

`--ids <file>` also converts the ids in that file (any integers in it, e.g. a boss sweep's id
list: SpEffectVfx of boss SpEffects, bullet chains, EMEVD effects the sources above miss).

Needs: `sekiro-extract unpack <Sekiro dir> extracted 'sfxbnd_(commoneffects|m[0-9])'`, combat_data.json,
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
# Where an id is looked up: the common archive first, then the areas' (an id in several areas is
# taken from the first; gap: the game loads the archive of the map it is on).
SFX_DIRS = [SFX] + sorted(glob.glob(os.path.join(ROOT, 'extracted/sfx/sfxbnd_m*.ffxbnd.d')))


def find(name):
    return next((os.path.join(s, name) for s in SFX_DIRS if os.path.exists(os.path.join(s, name))), None)
OUT = os.path.join(ROOT, 'extracted/fxr')
TEX = os.path.join(ROOT, 'extracted/fxr_tex')
# SEKIRO_EXTRACT (tools/extract.ps1 passes the one it used), else a source build: release, then debug.
EXTRACT = os.environ.get('SEKIRO_EXTRACT') or next(
    (p for p in (os.path.join(ROOT, f'tools/sekiro-extract/target/{b}/sekiro-extract.exe') for b in ('release', 'debug')) if os.path.exists(p)),
    os.path.join(ROOT, 'tools/sekiro-extract/target/release/sekiro-extract.exe'))
# The blood sprays gore.rs drives from the TAE, and the Sculptor's Idol's flame (o005000's TAE:
# 1011 lit, 1010 unlit, 1000 the lighting flash; src/idol.rs).
EXTRA = [220502, 220503, 220505, 220506, 1000, 1010, 1011]
# SpEffects put on Wolf from outside his own list (their SpEffectVfx effects).
SP_EFFECTS = [211000]


def used_ids():
    d = json.load(open(os.path.join(ROOT, 'extracted/combat_data.json'), encoding='utf-8'))
    ids = set(EXTRA)
    chars = [d['player']] + list(d['enemies'].values())
    for f in sorted(glob.glob(os.path.join(ROOT, 'extracted/enemies/c*.json'))):
        try:
            c = json.load(open(f, encoding='utf-8'))
        except (OSError, ValueError):
            continue
        if isinstance(c.get('anims'), dict):
            chars.append(c)
    for c in chars:
        for a in c['anims'].values():
            if not isinstance(a, dict):
                continue
            for e in a.get('events', []):
                args = e.get('args')
                # SpawnFFX_ByFloor (112): FFXID is a FootSfxParam column, added below.
                if e.get('type') != 112 and isinstance(args, dict) and isinstance(args.get('FFXID'), int) and args['FFXID'] > 0:
                    ids.add(args['FFXID'])
        for coll in ('bulletRows', 'bullets'):
            rows = c.get(coll) or {}
            for b in (rows.values() if isinstance(rows, dict) else rows):
                if isinstance(b, dict):
                    for k in ('sfxId_Bullet', 'sfxId_Hit', 'sfxId_Flick', 'sfxId_ForceErase'):
                        if isinstance(b.get(k), int) and b[k] > 0:
                            ids.add(b[k])
    # Status effects on a character (burning, poison): SpEffect vfxId / vfxId1 -> SpEffectVfxParam
    # initSfxId (once) / midstSfxId (while it lasts), src/status.rs.
    vfx = json.load(open(os.path.join(ROOT, 'extracted/json/params/SpEffectVfxParam.json'), encoding='utf-8'))['rows']
    vfx = {r['id']: r for r in (vfx.values() if isinstance(vfx, dict) else vfx)}
    # Plus the perilous warning on Wolf: SpEffect 211000 -> SpEffectVfx 48100 "Kanji" -> 600035
    # on dummy 220 (src/enemy_bullet.rs).
    sp = json.load(open(os.path.join(ROOT, 'extracted/json/params/SpEffectParam.json'), encoding='utf-8'))['rows']
    sp = {r['id']: r for r in (sp.values() if isinstance(sp, dict) else sp)}
    # And the SpEffects a character's TAE puts on itself (SpEffect event 67: the Demon of Hatred's
    # burning body 3702011 -> 670215 / 670216, the Guardian Ape 3510500 -> 651050), src/ffx.rs.
    tae_sp = set()
    for c in chars:
        for a in c['anims'].values():
            if isinstance(a, dict):
                for e in a.get('events', []):
                    if e.get('type') == 67 and isinstance((e.get('args') or {}).get('SpEffectID'), int):
                        tae_sp.add(e['args']['SpEffectID'])
    # The quick items (src/items.rs): each goods' own use effects (castSfxId / fireSfxId /
    # effectSfxId, TAE 114 SpawnFFX_GoodsAndMagic SFXIndexID 0 / 1 / 2, src/ffx.rs) and the
    # SpEffects it puts on Wolf (items.rs `effects_of`: its refId_default Bullet's, and that
    # bullet's HitBulletID chain's, spEffectIDForShooter: a sugar's glow).
    goods_sp = set()
    goods = json.load(open(os.path.join(ROOT, 'extracted/json/params/EquipParamGoods.json'), encoding='utf-8'))['rows']
    bullets = json.load(open(os.path.join(ROOT, 'extracted/json/params/Bullet.json'), encoding='utf-8'))['rows']
    bullets = {r['id']: r for r in (bullets.values() if isinstance(bullets, dict) else bullets)}
    for g in (goods.values() if isinstance(goods, dict) else goods):
        if g.get('goodsUseAnim', 255) == 255:
            continue
        for k in ('castSfxId', 'fireSfxId', 'effectSfxId'):
            if isinstance(g.get(k), int) and g[k] > 0:
                ids.add(g[k])
        # refCategory 2: refId_default is the SpEffect itself (the gourd's heal 3000).
        if g.get('refCategory') == 2 and g.get('refId_default', -1) in sp:
            goods_sp.add(g['refId_default'])
        b = g.get('refId_default', -1) if g.get('refCategory') == 1 else -1
        for _ in range(4):
            if b not in bullets:
                break
            s = bullets[b].get('spEffectIDForShooter', -1)
            if s in sp:
                goods_sp.add(s)
            n = bullets[b].get('HitBulletID', -1)
            b = -1 if n == b else n
    for s in list(d['player'].get('spEffects', {}).values()) + [sp.get(i, {}) for i in SP_EFFECTS] + [sp.get(i, {}) for i in sorted(tae_sp | goods_sp)]:
        for k in ('vfxId', 'vfxId1'):
            r = vfx.get(s.get(k, -1))
            for f in ('initSfxId', 'midstSfxId'):
                if r and r.get(f, -1) > 0:
                    ids.add(r[f])
    # Foot dust (TAE 792 FootSfxParam_Entity / 112 SpawnFFX_ByFloor, src/ffx.rs): every
    # FootSfxParam id of the rows the characters use (Wolf TentativePlayerParam
    # PlayerFootEffect_bySFX, NPCs NpcParam footEffectSfxId).
    def rows(name):
        r = json.load(open(os.path.join(ROOT, f'extracted/json/params/{name}.json'), encoding='utf-8'))['rows']
        return {x['id']: x for x in (r.values() if isinstance(r, dict) else r)}
    foot = rows('FootSfxParam')
    want = {r.get('PlayerFootEffect_bySFX', -1) for r in rows('TentativePlayerParam').values()}
    want |= {r.get('footEffectSfxId', -1) for r in d['params'].get('NpcParam', {}).values()}
    for w in want:
        for k, v in foot.get(w, {}).items():
            if k.startswith('sfxId_') and isinstance(v, int) and v > 0:
                ids.add(v)
    # Hit / guard / deflect sparks: every FXR id in HitEffectSfxParam (vfx.rs, combat.rs).
    for r in d['params'].get('HitEffectSfxParam', {}).values():
        for k, v in r.items():
            if k != 'id' and isinstance(v, int) and v > 0:
                ids.add(v)
    return ids


def dump(ids):
    by_dir = {}
    for i in sorted(ids):
        src = find(f'f{i:09d}.fxr')
        if src and not os.path.exists(os.path.join(OUT, f'{i}.json')):
            by_dir.setdefault(os.path.dirname(src), []).append(i)
    for sfx, ids in by_dir.items():
        for k in range(0, len(ids), 60):
            subprocess.run(['node', os.path.join(ROOT, 'tools/fxr-dump/dump.mjs'), sfx, OUT] + [str(i) for i in ids[k:k + 60]], check=True, stdout=subprocess.DEVNULL)


def main():
    os.makedirs(OUT, exist_ok=True)
    os.makedirs(TEX, exist_ok=True)
    ids = used_ids()
    if '--ids' in sys.argv:
        ids |= {int(x) for x in re.findall(r'\d+', open(sys.argv[sys.argv.index('--ids') + 1]).read()) if int(x) > 0}
    dump(ids)
    # References (2001 "sfx": id) until nothing new.
    while True:
        have = {int(os.path.basename(f)[:-5]) for f in glob.glob(os.path.join(OUT, '*.json'))}
        refs = set()
        for f in glob.glob(os.path.join(OUT, '*.json')):
            refs.update(int(x) for x in re.findall(r'"sfx": (\d+)', open(f).read()))
        new = {r for r in refs - have if find(f'f{r:09d}.fxr')}
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
        tpf = find(f's{t:05d}.tpf')
        if os.path.exists(png) or not tpf:
            continue
        d = open(tpf, 'rb').read()
        doff, dsz = struct.unpack_from('<Ii', d, 0x10)
        dds = os.path.join(TEX, f'{t}.dds')
        open(dds, 'wb').write(d[doff:doff + dsz])
        if subprocess.run([EXTRACT, 'dds2png', dds, png], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL).returncode == 0:
            done += 1
        os.remove(dds)
    print(f'{len(glob.glob(os.path.join(OUT, "*.json")))} effects, {len(tex)} textures ({done} new)')
    models()


def model_ids(v, out):
    if isinstance(v, dict):
        if v.get('type') == 605 and isinstance(v.get('model'), int) and v['model'] > 0:
            out.add(v['model'])
        for c in v.values():
            model_ids(c, out)
    elif isinstance(v, list):
        for c in v:
            model_ids(c, out)


def models():
    ids = set()
    for f in glob.glob(os.path.join(OUT, '*.json')):
        model_ids(json.load(open(f, encoding='utf-8')), ids)
    dst = os.path.join(ROOT, 'extracted/fxr_model')
    os.makedirs(dst, exist_ok=True)
    made = 0
    for mid in sorted(ids):
        out = os.path.join(dst, f'model_{mid}.bin')
        flver = find(f's{mid:05d}.flver')
        if os.path.exists(out) or not flver:
            continue
        names = [m.decode('utf-16le') for m in re.findall(rb'\\\x00((?:[a-zA-Z0-9_]\x00)+)\.\x00[tT]\x00[iI]\x00[fF]\x00', open(flver, 'rb').read())]
        diffuse = next((t for t in names if t.lower().endswith('_a')), None) or (f's{mid:05d}' if f's{mid:05d}' in names else None)             or next((t for t in names if not re.search(r'_[nrm]$', t.lower())), None)
        tpf = diffuse and find(f'{diffuse}.tpf')
        if not tpf:
            continue
        if subprocess.run([EXTRACT, 'model', flver, tpf, out, os.path.join(ROOT, 'extracted/tex')], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL).returncode == 0:
            made += 1
    print(f'{len(ids)} effect models ({made} new)')


if __name__ == '__main__':
    sys.exit(main())
