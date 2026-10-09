"""One-off: split docs/PROGRESS.md into docs/kb/<topic>.md files plus a short index.

Log entries ("- 2026-..." bullets with their indented continuation lines) go to the topic whose
keywords they mention most. The untouched original is kept as docs/kb/log-archive.md.
"""
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SRC = ROOT / "docs" / "PROGRESS.md"
KB = ROOT / "docs" / "kb"

TOPICS = {
    "input-buffer": ("Input buffer, accept/execute flags, FireEvent resets", ["buffer", "accept", "actionrequest", "fireevent", "keeps_buffer", "execute flag", "latch"]),
    "attacks": ("Wolf's attacks: combos, releases, air/sprint/step attacks, combat arts, lunge, auto-homing", ["attack", "combo", "slash", "release", "whirlwind", "combat art", "lunge", "760", "homing", "auto-aim", "hit stop", "swept"]),
    "guard-deflect": ("Guard, deflect, posture, mikiri, deathblow, perilous attacks", ["deflect", "guard", "posture", "mikiri", "deathblow", "perilous", "stagger", "parry"]),
    "damage": ("Damage reactions, knockback, breaks, invulnerability, death and revival", ["damage", "knockback", "knock-down", "knockdown", "reaction", "revival", "resurrect", "invulnerab", "950", "951", "hp "]),
    "movement": ("Locomotion, turning, quick turns, sprint, step, jump, air control, kick", ["jump", "sprint", "quick turn", "turn", "locomotion", "air control", "kick", "step", "walk", "run "]),
    "camera": ("Camera: LockCamParam/CameraParam, lock-on, TAE 151/153/155", ["camera", "lockcam", "cameraparam", "pitch", "fov", "lock-on", "tae 153", "tae 155", "tae 151"]),
    "enemy": ("Enemies: real Lua AI, NpcParam, c1020/c1010, enemy reactions", ["enemy", " ai", "c1010", "c1020", "npcparam", "ochimusha", "lua", "think"]),
    "visuals": ("Models, textures, animation playback, blending, behaviour graph, lighting", ["model", "mesh", "flver", "texture", "animation", "clip", "blend", "hkx", "skeleton", "lighting", "dummy", "visual", "tpf"]),
    "sound": ("Sound: FSB/FMOD decoding, sound events", ["sound", "fsb", "fmod", "fev", "audio"]),
    "tools-exe": ("Extractor, Ghidra/decompile, gamedb, RTTI, exe unpacking", ["ghidra", "gamedb", "rtti", "decompile", "extractor", "dearxan", "steamless", "sekiro-extract", "arxan"]),
}


def topic_of(text: str) -> str:
    t = text.lower()
    scores = {k: sum(t.count(w) for w in kws) for k, (_, kws) in TOPICS.items()}
    best = max(scores, key=scores.get)
    return best if scores[best] > 0 else "tools-exe"


def main() -> None:
    lines = SRC.read_text(encoding="utf-8").splitlines()
    KB.mkdir(parents=True, exist_ok=True)
    (KB / "log-archive.md").write_text("# Full log archive (pre-split PROGRESS.md, for grep only)\n\n" + "\n".join(lines) + "\n", encoding="utf-8")

    head_end = next(i for i, l in enumerate(lines) if l.startswith("## Log"))
    next_start = next(i for i, l in enumerate(lines) if l.startswith("## NEXT"))
    header = lines[:head_end]

    # NEXT list: from "## NEXT" until the first log bullet after it.
    nxt, i = [], next_start + 1
    while i < len(lines) and not lines[i].startswith("- 20"):
        nxt.append(lines[i])
        i += 1
    rest = lines[head_end + 1:next_start] + lines[i:]

    # Group into entries: a line starting at column 0 opens an entry; indented lines continue it.
    entries, cur = [], []
    for l in rest:
        if l and not l.startswith(" ") and cur:
            entries.append(cur)
            cur = []
        if l.strip():
            cur.append(l)
    if cur:
        entries.append(cur)
    # The old file kept two log segments; order entries by their date (stable within a day).
    date = lambda e: (re.match(r"- (\d{4}-\d{2}-\d{2})", e[0]) or re.match(r"()", "")).group(1) or "9999"
    entries.sort(key=date)

    by_topic = {k: [] for k in TOPICS}
    open_items = []
    for e in entries:
        text = "\n".join(e)
        if re.match(r"^(SWORDPLAY|VISUALS) NEXT", e[0]) or "NEXT (for the next session)" in text:
            open_items.append(text)
        by_topic[topic_of(text)].append(text)

    for k, (desc, _) in TOPICS.items():
        body = f"# {k}: {desc}\n\nDated findings, oldest first. Append new entries at the end.\n\n" + "\n".join(by_topic[k]) + "\n"
        (KB / f"{k}.md").write_text(body, encoding="utf-8")
    (KB / "next.md").write_text("# NEXT list (history and open items)\n\n" + "\n".join(nxt).strip() + "\n\n## Open items from the log\n\n" + "\n".join(open_items) + "\n", encoding="utf-8")

    index = header + [
        "## Knowledge base (read only the topic you need)",
        "Detailed findings live in docs/kb/. Each file is dated entries, oldest first. Add new findings to the",
        "matching topic file, and one line to Recent below (keep the last 15).",
        "",
    ] + [f"- [kb/{k}.md](kb/{k}.md) - {d} ({len(by_topic[k])} entries)" for k, (d, _) in TOPICS.items()] + [
        "- [kb/next.md](kb/next.md) - NEXT list history and open items",
        "- [kb/log-archive.md](kb/log-archive.md) - the full pre-split log (grep it, don't read it whole)",
        "- [HANDOFF_PLAN.md](HANDOFF_PLAN.md) - task packets for helper models",
        "",
        "## Recent",
    ] + [f"- {e[0][2:120]}" for e in entries[-15:]] + [""]
    SRC.write_text("\n".join(index) + "\n", encoding="utf-8")
    for k in TOPICS:
        print(f"{k:14} {len(by_topic[k]):3}")
    print("open items", len(open_items), "| PROGRESS.md lines", len(index))


if __name__ == "__main__":
    main()
