"""MSB placement groups: extracted/enemies/placement_groups.json.

{map: {entity id: [group, ...]}}: the enemies each MSBS event of type 21 PlacementGroup lists
(type data: 32 x i32 part indices into PARTS_PARAM_ST, -1 unused; SoulsFormats MSBS
EventParam.cs PlacementGroup), a group being the event's ordinal among the type-21 events. The
exe fills a per-ChrSet table (ChrSet+0xb8, 8 bytes per enemy slot, 0xFF empty) with those
ordinals (FUN_140a42c70) and an AiSoundParam row with bRestrictSamePlacementGroup (the
call-friend sounds) reaches a listener only through a shared one (FUN_140610810; src/enemy.rs
ai_sound_heard). Enemies in no group are left out.
Usage: python tools/placement_groups.py
"""
import glob
import json
import os
import struct
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import boss_scripts as bs  # noqa: E402
from patrols import tables  # noqa: E402


def groups(path):
    d = open(path, 'rb').read()
    t = tables(d)
    eids = []
    for o in t['PARTS_PARAM_ST']:
        eo = struct.unpack_from('<q', d, o + 0x60)[0]
        eids.append(struct.unpack_from('<i', d, o + eo)[0])
    out, n = {}, 0
    for o in t.get('EVENT_PARAM_ST', ()):
        typ = struct.unpack_from('<i', d, o + 0xC)[0]
        if typ != 21:
            continue
        td = o + struct.unpack_from('<q', d, o + 0x20)[0]
        for i in struct.unpack_from('<32i', d, td):
            if 0 <= i < len(eids) and eids[i] > 0:
                out.setdefault(str(eids[i]), []).append(n)
        n += 1
    return out


def main():
    out = {}
    for f in sorted(glob.glob(os.path.join(bs.ROOT, 'map', 'mapstudio', 'm*.msb'))):
        g = groups(f)
        if g:
            out[os.path.basename(f)[:-4]] = g
            print(f'{os.path.basename(f)}: {len(g)} grouped enemies')
    path = os.path.join(bs.ROOT, 'enemies', 'placement_groups.json')
    with open(path, 'w', encoding='utf-8', newline='\n') as f:
        json.dump(out, f, indent=1, ensure_ascii=False)
    print(f'{path}: {sum(len(v) for v in out.values())} enemies')


if __name__ == '__main__':
    main()
