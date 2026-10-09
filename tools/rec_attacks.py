"""Posture / HP hits on Wolf matched to the attacking enemy's TAE attack event and its AtkParam_Npc row.
Enemy TAE must be decoded: sekiro-extract tae extracted/chr/cXXXX.anibnd.d extracted/json/tae_cXXXX
  python tools/rec_attacks.py <rec_dir> cXXXX"""
import json, struct, sys
sys.path.insert(0, 'tools')
import rec_read as rr

def load(n):
    d = json.load(open(f'extracted/json/params/{n}.json', encoding='utf-8'))
    rows = d if isinstance(d, list) else d.get('rows', d)
    return {int(r['id']): r for r in rows} if isinstance(rows, list) else {int(k): v for k, v in rows.items()}

def main():
    rec, chr_ = sys.argv[1], sys.argv[2]
    names, by, recs = rr.load(rec)
    tae = json.load(open(f'extracted/json/tae_{chr_}/{chr_}.json', encoding='utf-8'))
    A = {e['id']: e for e in tae['anims']}
    lo = int(chr_[1:]) * 10000
    NPC, BEH, ATK = load('NpcParam'), load('BehaviorParam'), load('AtkParam_Npc')
    dm, ta = recs[(0, by['data'])], recs[(0, by['timeact'])]
    ticks = sorted(t for t in dm if t in ta)
    post = {t: struct.unpack_from('<i', dm[t][1], 0x48)[0] for t in ticks}
    hp = {t: struct.unpack_from('<i', dm[t][1], 0x30)[0] for t in ticks}
    me = rr.body_anims(ta)

    def slot_info(slot):
        sta, sch = recs[(slot, by['timeact'])], recs[(slot, by['chr_head'])]
        info, cur, start = {}, None, None
        for t in sorted(sta):
            a = struct.unpack_from('<i', sta[t][1], 0x100)[0]
            npc = struct.unpack_from('<i', sch[t][1], 0x68)[0] if t in sch else 0
            if (a, npc) != cur:
                cur, start = (a, npc), t
            info[t] = (a, npc, (sta[t][0] - sta[start][0]) / 1000)
        return info

    S = {s: slot_info(s) for s in (1, 2)}
    print("anim judge AtkParam fields | outcome | Wolf posture / hp loss | npc")
    for i in range(1, len(ticks)):
        t, p = ticks[i], ticks[i - 1]
        dp = post[p] - post[t]
        if dp <= 1:
            continue
        for s in (1, 2):
            if t not in S[s]:
                continue
            a, npc, at = S[s][t]
            if not (lo <= npc < lo + 100000):
                continue
            ev = A.get(a) or A.get(a % 1000000)
            hits = [e for e in ev['events'] if e['type'] == 1 and e['start'] - 0.1 <= at <= e['end'] + 0.1] if ev else []
            if not hits:
                continue
            j = hits[0]['args'].get('BehaviorJudgeID')
            var = NPC.get(npc, {}).get('behaviorVariationId')
            b = BEH.get(200_000_000 + var * 1000 + j) if var is not None and j is not None else None
            atk = ATK.get(b['refId']) if b and b.get('refType') == 0 else None
            nxt = me[ticks[min(i + 3, len(ticks) - 1)]]
            grp = (nxt % 1000000) // 10000 if nxt // 1000000 == 50 else -1
            kind = 'HIT' if hp[t] < hp[p] else {13: 'DEFLECT', 12: 'BLOCK'}.get(grp, f'other a{nxt // 1000000:03d}_{nxt % 1000000:06d}')
            g = lambda f: atk and atk.get(f)
            w = me[ticks[max(0, i - 2)]]
            print(f"wolf a{w // 1000000:03d}_{w % 1000000:06d} | {a % 1000000:6d} {j} {b and b.get('refId')} stam {g('atkStam')} direct {g('directAtkStamDamage')} repelLost {g('repelLostStamDamage')} phys {g('atkPhys')}  {kind:7s} posture -{dp:<4d} hp -{hp[p] - hp[t]:<4d} {npc}")
            break

main()
