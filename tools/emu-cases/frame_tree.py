"""What the game runs that Sv1 has not ported, from an `emu-cases trace` run.

Usage: python tools/emu-cases/frame_tree.py <out.md> <trace.json> [min_bytes] [--base <trace.json>]... [--show <addr>]...

With --base (e.g. the idle acttrace), only functions the base traces did not enter count ("new"):
the tree keeps the paths down to them, and a list of the new branches' entry points (a new function
whose caller is not new) follows, by new bytes below them. --show <addr> adds that function's
subtree (from its place in the tree, six levels down) after the list.

Puts each function the trace entered under its shallowest caller (from the trace's return
addresses), adds up each subtree (functions, bytes) and how many of them Sv1 cites: an address
(FUN_<addr> or 0x14xxxxxxx) written anywhere in src/ or docs/kb. Citing is a proxy: a cited function
may be partly ported, and a ported one may be uncited. Names come from the exe's RTTI vtables
(DSekiro data/symbols.txt). Needs DSekiro's data/functions.json and image/sekiro.exe (local only).
"""
import bisect
import json
import os
import re
import struct
import sys
from collections import defaultdict, deque
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
# A DSekiro checkout (its data/ and image/sekiro.exe): set DSEKIRO.
DSEKIRO = Path(os.environ.get('DSEKIRO') or sys.exit('set DSEKIRO to a DSekiro checkout'))
BASE = 0x140000000


def cited_addresses():
    pat = re.compile(r'FUN_(1[0-9a-fA-F]{8})|0x(14[0-9a-fA-F]{7})\b')
    out = set()
    for root in (REPO / 'src', REPO / 'docs' / 'kb'):
        for p in root.rglob('*'):
            if p.suffix in ('.rs', '.md'):
                for m in pat.finditer(p.read_text(encoding='utf-8', errors='replace')):
                    out.add(int(m.group(1) or m.group(2), 16))
    return out


def vtable_names(exe):
    pe = struct.unpack_from('<I', exe, 0x3c)[0]
    nsec, opt = struct.unpack_from('<H', exe, pe + 6)[0], struct.unpack_from('<H', exe, pe + 20)[0]
    secs = [struct.unpack_from('<8sIIII', exe, pe + 24 + opt + 40 * i) for i in range(nsec)]

    def off(va):
        rva = va - BASE
        for _, _, v, rs, raw in secs:
            if v <= rva < v + rs:
                return raw + rva - v
    names = {}
    for line in open(DSEKIRO / 'data' / 'symbols.txt', encoding='utf-8', errors='replace'):
        m = re.match(r'\?\?_7(.+?)@@6B@.*=\s*(0x[0-9a-fA-F]+)', line)
        o = m and off(int(m.group(2), 16))
        if not o:
            continue
        cls = m.group(1).split('@')[0].lstrip('?$')
        for i in range(200):
            f = struct.unpack_from('<Q', exe, o + 8 * i)[0]
            if not (BASE + 0x1000 <= f < BASE + 0x3000000):
                break
            names.setdefault(f, f'{cls}::vf{i}')
    return names


def main():
    argv, bases, shows = [], [], []
    it = iter(sys.argv[1:])
    for a in it:
        if a == '--base':
            bases.append(next(it))
        elif a == '--show':
            shows.append(int(next(it), 16))
        else:
            argv.append(a)
    out_path, trace_path = argv[0], argv[1]
    min_bytes = int(argv[2]) if len(argv) > 2 else 8000
    base = set()
    for b in bases:
        base |= {int(e['va'], 16) for e in json.load(open(b))['entered']}
    cited = cited_addresses()
    combat = {int(l.split()[0], 16) for l in open(DSEKIRO / 'data' / 'targets' / 'combat.txt') if l.startswith('0x')}
    names = vtable_names((DSEKIRO / 'image' / 'sekiro.exe').read_bytes())
    funcs = sorted((int(f['va'], 16), f['size']) for f in json.load(open(DSEKIRO / 'data' / 'functions.json')))
    starts = [f[0] for f in funcs]

    def func_of(a):
        i = bisect.bisect_right(starts, a) - 1
        return funcs[i][0] if i >= 0 and a < funcs[i][0] + max(funcs[i][1], 1) else None

    t = json.load(open(trace_path))
    root = int(t['function'], 16)
    # A run of several calls (acttrace's duel: world tasks and two chr frames) lists them in
    # run.roots; they hang under a made-up root 0x0.
    roots = [int(r, 16) for r in t.get('run', {}).get('roots', [])]
    if roots:
        root = 0
    size = {int(e['va'], 16): e['size'] for e in t['entered']}
    kids = defaultdict(set)
    for r, f in t['edges']:
        c, f = func_of(int(r, 16)), int(f, 16)
        if c is not None and c != f:
            kids[c].add(f)
    kids[0] |= set(roots)
    parent, queue = {root: None}, deque([root])
    while queue:
        n = queue.popleft()
        for k in sorted(kids[n]):
            if k not in parent:
                parent[k] = n
                queue.append(k)
    tree = defaultdict(list)
    for n, p in parent.items():
        if p is not None:
            tree[p].append(n)
    def new(n):
        return not bases or n not in base
    agg = {}
    for n in reversed(list(parent)):  # BFS order reversed: children before parents
        s, c, k = (size.get(n, 0), int(n in cited), 1) if new(n) else (0, 0, 0)
        for ch in tree[n]:
            s, c, k = s + agg[ch][0], c + agg[ch][1], k + agg[ch][2]
        agg[n] = (s, c, k)
    lines = [f'# Call tree of {t["function"]} ({Path(trace_path).name})', '',
             f'{len(parent)} functions reached ({len(size)} entered). Each line: function, own size, subtree '
             f'(functions / bytes / cited by Sv1). Subtrees under {min_bytes:,} bytes are folded.', '']
    if bases:
        lines[2:2] = [f'Base: {", ".join(Path(b).name for b in bases)}. Only functions the base did not enter '
                      f'are counted: {agg[root][2]} new, {agg[root][0]:,} bytes, {agg[root][1]} cited; '
                      f'`new` marks them, the others are the path down.', '']

    def show(n, d, top=0):
        s, c, k = agg[n]
        mark = (' **cited**' if n in cited else '') + (' new' if bases and new(n) else '')
        extra = ' (combat list)' if n in combat else ''
        lines.append(f'{"  " * d}- `{n:#x}` {names.get(n, "")}{mark}: own {size.get(n, 0)} B, subtree {k} fn / {s:,} B / {c} cited{extra}')
        for ch in sorted(tree[n], key=lambda x: -agg[x][0]):
            if agg[ch][0] >= min_bytes and d < top + 6:
                show(ch, d + 1, top)
    show(root, 0)
    if bases:
        heads = [n for n in parent if new(n) and parent[n] is not None and not new(parent[n])]
        heads.sort(key=lambda n: -agg[n][0])
        lines += ['', '## New branches (entry points, by new bytes below)', '']
        for n in heads:
            s_, c, k = agg[n]
            if s_ < min_bytes / 4:
                continue
            lines.append(f'- `{n:#x}` {names.get(n, "")}{" **cited**" if n in cited else ""} (from `{parent[n]:#x}` '
                         f'{names.get(parent[n], "")}): {k} fn / {s_:,} B / {c} cited')
    for n in shows:
        lines += ['', f'## Subtree of `{n:#x}`', '']
        if n in agg:
            show(n, 0)
    Path(out_path).write_bytes(('\n'.join(lines) + '\n').encode('utf-8'))
    print(f'{len(parent)} reached, {agg[root][1]} cited; {len(lines)} lines -> {out_path}')


if __name__ == '__main__':
    main()
