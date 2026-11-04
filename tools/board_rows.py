"""Every enemy and boss gets a board row: lists the fighting characters in docs/kb/coverage.md that no row on the
board names yet (open or Done), as ready rows for the "C" section.

  python tools/board_rows.py                    # print the missing rows
  python tools/board_rows.py --write WORK.md    # add them to the C section (inside the board worktree)

A fighter is a boss or mini-boss (the map's health bar), or a character with an AI battle goal that is neither a
talking NPC (c7xxx, "[Conversation]") nor a dummy. Run tools/coverage.py first when new characters were extracted.
"""
import os
import re
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


def cells(line):
    return [c.strip() for c in line.strip().strip("|").split("|")]


def fighters():
    """[(chr, name, kind, proof level)] in coverage.md's order."""
    out, section = [], None
    for line in open(os.path.join(ROOT, "docs", "kb", "coverage.md"), encoding="utf-8"):
        if line.startswith("## "):
            section = line[3:].strip()
        c = cells(line)
        if section != "Characters" or not re.match(r"c\d{4}$", c[0]) or c[0] == "c0000" or len(c) < 14:
            continue
        name, kind, ai, level = re.sub(r"\s*\(param row name\)$", "", c[1]), c[2], c[6], c[-1]
        npc = (c[0].startswith("c7") and not kind) or "conversation" in name.lower() or "dummy" in name.lower()
        if kind in ("boss", "mini-boss") or (re.match(r"\d+/\d+ goals", ai) and not npc):
            out.append((c[0], name, kind, level))
    return out


def board_text():
    r = subprocess.run(["git", "remote"], cwd=ROOT, capture_output=True, text=True)
    remote = "upstream" if "upstream" in r.stdout.split() else "origin"
    r = subprocess.run(["git", "show", f"{remote}/agents-work-board:WORK.md"], cwd=ROOT, capture_output=True,
                       text=True, encoding="utf-8")
    if r.returncode:
        sys.exit(f"cannot read WORK.md: git fetch {remote} agents-work-board")
    return r.stdout


def missing(text):
    named = set(re.findall(r"\bc\d{4}\b", "\n".join(l for l in text.splitlines() if l.startswith("| "))))
    ids = [int(m) for m in re.findall(r"^\| C(\d+)[a-z]? \|", text, re.M)]
    n, rows = max(ids, default=0), []
    order = {"boss": 0, "mini-boss": 1}
    for chr_id, name, kind, level in sorted(fighters(), key=lambda f: order.get(f[2], 2)):
        if chr_id in named:
            continue
        n += 1
        names = name.split("; ")
        name = "; ".join(names[:2]) + ("; ..." if len(names) > 2 else "")
        what = {"boss": "Per-boss check", "mini-boss": "Per-miniboss check"}.get(kind, "Per-enemy check")
        rows.append(f"| C{n} | emu | | open | {what}: {name} ({chr_id}): its attacks, reactions and AI goals against "
                    f"the exe, with a scene where it stands | the C section's steps; now {level} |")
    return rows


def main():
    a = sys.argv[1:]
    if a[:1] == ["--write"] and len(a) == 2:
        text = open(a[1], encoding="utf-8").read()
        rows = missing(text)
        lines = text.splitlines()
        sec = next((i for i, l in enumerate(lines) if l.startswith("## C:")), None)
        if sec is None:
            sys.exit("no '## C:' section in " + a[1])
        end = next((i for i in range(sec + 1, len(lines)) if lines[i].startswith("## ")), len(lines))
        while lines[end - 1].strip() == "":
            end -= 1
        lines[end:end] = rows
        open(a[1], "w", encoding="utf-8", newline="\n").write("\n".join(lines) + "\n")
        print(f"added {len(rows)} rows to {a[1]}")
    elif not a:
        rows = missing(board_text())
        print("\n".join(rows) or "every fighting character has a row")
    else:
        sys.exit(__doc__)


if __name__ == "__main__":
    main()
