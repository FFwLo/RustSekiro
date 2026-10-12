"""Armoured enemies: the EMEVD NPC parts that make an enemy's armour (common_func 20005410).

extracted/enemies/armor_parts.json: {NpcParam row: {part_group, part_hp, damage_correction,
body_rate, prosthetic, sp_effects, break_anim, after}}, read by src/enemy.rs (armor_of).

common_func 20005410 (the c1180 servants' armour; m10_00 1000363 = NpcParam 11807101, m11_01
1110432/33 = 11801100, 1110435 = 11806010): Create NPC Part (2004[22]: part group 2 = the
PelvisArmor ragdoll bodies, part HP 1, damage correction 1.0, body damage compensation 0.7),
Set NPC Part Damaging Prosthetic (2004[62]: 11 = Loaded Spear), Set SpEffect 220061 / 220062 /
400250 (2004[8]); on part HP 0 (4[6]): anim 20039 (2003[18]), those cleared (2004[21]), 230400 /
230401 set, the part re-made at HP 999999999 with no cut. Only rows whose every MSB entity runs
the event are written (all four do), and none of the boss rows (npc_names.json "rows": c1070
10701000's 20005411 is a boss's). Usage: python tools/armor_parts.py
"""
import glob
import json
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import boss_events as be  # noqa: E402
import boss_scripts as bs  # noqa: E402

ARMOR_EVENTS = (20005410, 20005411)
# Every cut / posture rate the SDT SpEffectParam layout has per attack attribute (Paramdex
# ATKPARAM_ATKATTR_TYPE / ATKPARAM_STAMINA_ATKATTR_TYPE; src/data.rs damage_cut_field / stamina_rate_field).
RATE_FIELDS = [f'{k}DamageCutRate' for k in ('slash', 'lightHit', 'thrust', 'neutral', 'attriA', 'attriB', 'attriC')] + \
    [f'def{k}DmgRate' for k in ('Ninsatsu', 'HeavyHit', 'AntiGround', 'AntiAir', 'LightShoot')] + \
    [f'def{k}StaminaDmgRate' for k in ('Slash', 'LightHit', 'Thrust', 'Neutral', 'Ninsatu', 'HeavyHit', 'AntiGround',
                                       'AntiAir', 'LightShoot', 'AttriA', 'AttriB', 'AttriC')]


def main():
    types = bs.emedf()
    common = bs.emevd(os.path.join(bs.ROOT, 'event', 'common_func.emevd'))
    names = os.path.join(bs.ROOT, 'enemies', 'npc_names.json')
    bosses = set(json.load(open(names, encoding='utf-8')).get('rows', {})) if os.path.exists(names) else set()
    rows, entities_of = {}, {}
    for f in sorted(glob.glob(os.path.join(bs.ROOT, 'event', 'm*_00_00.emevd'))):
        mp = os.path.basename(f)[:-6]
        msb = os.path.join(bs.ROOT, 'map', 'mapstudio', mp + '.msb')
        if not os.path.exists(msb):
            continue
        parts = {eid: npc for eid, _, npc in be.msb_parts(msb)}
        for eid, npc in parts.items():
            entities_of.setdefault(npc, set()).add(eid)
        for ev in bs.instances(bs.emevd(f), common, types):
            if ev['id'] not in ARMOR_EVENTS:
                continue
            ins = ev['ins']
            made = [v for b, i, v in ins if (b, i) == (2004, 22)]
            if not made:
                continue
            ent, _, group, hp, dmg, body = made[0][:6]
            npc = parts.get(ent)
            if npc is None or str(npc) in bosses:
                continue
            first_clear = next((k for k, (b, i, _) in enumerate(ins) if (b, i) == (2004, 21)), len(ins))
            entry = {
                'part_group': group, 'part_hp': hp, 'damage_correction': dmg, 'body_rate': body,
                'prosthetic': next((v[2] for b, i, v in ins if (b, i) == (2004, 62)), 0),
                'sp_effects': [v[1] for b, i, v in ins[:first_clear] if (b, i) == (2004, 8)],
                'break_anim': next((v[1] for b, i, v in ins if (b, i) == (2003, 18)), 0),
                'after': [v[1] for b, i, v in ins[first_clear:] if (b, i) == (2004, 8)],
                'entities': [],
            }
            rows.setdefault(npc, entry)['entities'].append(ent)
    # The SpEffects' HP cut and posture rates per attack attribute (SpEffectParam), multiplied.
    sp = json.load(open(os.path.join(bs.ROOT, 'json', 'params', 'SpEffectParam.json'), encoding='utf-8'))
    sp = {r['id']: r for r in (sp if isinstance(sp, list) else sp['rows']) if isinstance(r, dict)}
    out = {}
    for npc, entry in sorted(rows.items()):
        rates = {}
        for f in RATE_FIELDS:
            v = 1.0
            for i in entry['sp_effects']:
                v *= float(sp.get(i, {}).get(f, 1.0))
            rates[f] = round(v, 6)
        entry['rates'] = rates
        if set(entry['entities']) != entities_of.get(npc, set()):
            print(f'{npc}: not every entity is armoured ({sorted(entry["entities"])} of {sorted(entities_of.get(npc, ()))}), skipped')
            continue
        out[str(npc)] = entry
        print(f'{npc}: {entry}')
    path = os.path.join(bs.ROOT, 'enemies', 'armor_parts.json')
    with open(path, 'w', encoding='utf-8', newline='\n') as f:
        json.dump(out, f, indent=1)
    print(f'{path}: {len(out)} rows')


if __name__ == '__main__':
    main()
