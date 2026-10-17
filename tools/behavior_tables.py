"""Wolf's behaviour-graph tables -> extracted/blend_src.json and extracted/loop_clips_c0000.json.

- blend_src.json (src/data.rs blend_keeps_src): per graph state, whether a transition into it
  keeps the source clip's root motion through the blend. A transition effect with Havok flag 1
  (FLAG_IGNORE_FROM_WORLD_FROM_MODEL: StateToStateBlend, TaeBlend_NoSrcMotion*, read by
  `sekiro-extract transitions`) takes the to generator's root at once; the others keep the
  source moving. Each state's transitions in (`sekiro-extract sms`: "[ev E -> target EFFECT]"
  on the from states and the wildcards) are tallied, the majority decides (a tie keeps).
- loop_clips_c0000.json (src/data.rs loops_c0000): the clip generators in MODE_LOOPING
  (`sekiro-extract clips` with ALL_CLIPS=1, mode=1).

Both were made once by hand (2026-10, docs/history) and were missing from a fresh extract,
which kept every transition's source motion (the step -> sprint start dipped to 3 m/s).

Usage: python tools/behavior_tables.py
"""
import collections
import json
import os
import re
import subprocess

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.join(HERE, '..', 'extracted')
EXE = os.path.join(HERE, 'sekiro-extract', 'sekiro-extract.exe')
HKX = os.path.join(ROOT, 'chr', 'c0000.behbnd.d', 'c0000.hkx')


def run(*args, env=None):
    e = dict(os.environ)
    e.update(env or {})
    return subprocess.run([EXE, *args], capture_output=True, text=True, encoding='utf-8', errors='replace', env=e, check=True).stdout


def effect_flags():
    flags = {}
    for line in run('transitions', HKX).splitlines():
        m = re.match(r'\S+ (\S+) duration=\S+ flags=(\d+)', line)
        if m:
            flags[m.group(1)] = int(m.group(2))
    return flags


def blend_src(flags):
    tally = collections.defaultdict(lambda: [0, 0])  # state -> [keep, drop]
    sm_states, sm_moves = {}, []

    def flush():
        for target, effect in sm_moves:
            name = sm_states.get(target)
            if name is None:
                continue
            drop = flags.get(effect, 0) & 1
            tally[name][1 if drop else 0] += 1

    for line in run('sms', HKX).splitlines():
        if line and not line.startswith(' '):
            flush()
            sm_states, sm_moves = {}, []
            continue
        m = re.match(r'\s+(\S+) id=(\d+) ->', line)
        if m:
            sm_states[int(m.group(2))] = m.group(1)
        for t, eff in re.findall(r'\[ev \d+ -> (\d+) (\S+)\]', line):
            sm_moves.append((int(t), eff))
    flush()
    return {name: keep >= drop for name, (keep, drop) in sorted(tally.items())}


def loop_clips():
    out = []
    for line in run('clips', HKX, env={'ALL_CLIPS': '1'}).splitlines():
        m = re.match(r'(\S+) .*mode=1$', line)
        if m:
            out.append(m.group(1))
    return sorted(set(out))


def main():
    table = blend_src(effect_flags())
    with open(os.path.join(ROOT, 'blend_src.json'), 'w', encoding='utf-8') as f:
        json.dump(table, f, indent=0, sort_keys=True)
    loops = loop_clips()
    with open(os.path.join(ROOT, 'loop_clips_c0000.json'), 'w', encoding='utf-8') as f:
        json.dump(loops, f, indent=0)
    keep = sum(table.values())
    print(f'blend_src.json: {len(table)} states ({keep} keep the source motion, {len(table) - keep} drop it)')
    print(f'loop_clips_c0000.json: {len(loops)} looping clips')


if __name__ == '__main__':
    main()
