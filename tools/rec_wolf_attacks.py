"""Enemy HP / posture drops matched to Wolf's active TAE attack (BehaviorJudgeID -> our exported attack row).
  python tools/rec_wolf_attacks.py <rec_dir>"""
import json, struct, sys
sys.path.insert(0, 'tools')
import rec_read as rr

def main():
    names, by, recs = rr.load(sys.argv[1])
    d = json.load(open('extracted/combat_data.json', encoding='utf-8'))
    P = d['player']
    wta = recs[(0, by['timeact'])]
    wt = sorted(wta)
    WBA = rr.body_anims(wta)
    wanim, wstart, cur = {}, {}, None
    for t in wt:
        a = WBA[t]
        if a != cur:
            cur, s0 = a, t
        wanim[t], wstart[t] = a, (wta[t][0] - wta[s0][0]) / 1000
    print("Wolf anim     judge atkStam direct phys | enemy hp loss / posture loss | enemy anim after | npc")
    for slot in (1, 2):
        dm, ta, ch = recs[(slot, by['data'])], recs[(slot, by['timeact'])], recs[(slot, by['chr_head'])]
        ticks = sorted(t for t in dm if t in ta and t in ch and t in wanim)
        for p, t in zip(ticks, ticks[1:]):
            if struct.unpack_from('<i', ch[t][1], 0x68)[0] != struct.unpack_from('<i', ch[p][1], 0x68)[0]:
                continue
            hp0, hp1 = (struct.unpack_from('<i', dm[x][1], 0x30)[0] for x in (p, t))
            po0, po1 = (struct.unpack_from('<i', dm[x][1], 0x48)[0] for x in (p, t))
            if hp1 >= hp0 and po1 >= po0 - 1:
                continue
            a = wanim[t]
            k = f"a{a // 1000000:03d}_{a % 1000000:06d}"
            ev = P['anims'].get(k)
            hit = [e for e in (ev['events'] if ev else []) if e['type'] == 1 and e['start'] - 0.1 <= wstart[t] <= e['end'] + 0.1]
            j = hit[0]['args'].get('BehaviorJudgeID') if hit else None
            row = P['attacks'].get(str(j), {}) if j is not None else {}
            nxt = struct.unpack_from('<i', ta[ticks[min(ticks.index(t) + 3, len(ticks) - 1)]][1], 0x100)[0]
            npc = struct.unpack_from('<i', ch[t][1], 0x68)[0]
            print(f"{k} {j} {row.get('atkStam')} {row.get('directAtkStamDamage')} {row.get('atkPhys')} | -{hp0 - hp1} / -{po0 - po1} | a{nxt // 1000000:03d}_{nxt % 1000000:06d} | {npc}")

main()
