"""Scripted input for a recording: named input scripts -> the raw ops the hook DLL plays (tools/rec-hooks/src/play.rs).

Script (tools/rec/inputs/*.txt), one line each, `#` comments, frames at the game's 60 per second:
  <frame> stick <x> <y>          camera-relative stick, x right, y forward (0 0 = let go)
  <frame> press <action>         hold the button from this frame
  <frame> release <action>       let go
  <frame> tap <action> [frames]  press, release after `frames` (default 2)
  <frame> free                   hand everything back to the real pad
  <frame> end
Actions = ActionRequest bits (docs/kb/rec-research/input-hks.md A/B; names reference/SekiroHKS c0000_define.hks).
The combat art has no button of its own: press guard, then tap attack. Lock-on is not an action bit (not playable yet).
"""
from pathlib import Path

ACTIONS = {
    "attack": 0, "prosthetic": 1, "guard": 2, "grapple": 3, "jump": 4, "sprint": 5, "change_style": 6,
    "item": 7, "switch_form": 8, "weapon_r": 9, "weapon_l": 10, "change_item": 11, "wall_hang": 12,
    "backstep": 13, "step": 14, "crouch": 15, "eavesdrop": 31,
}
HELD = "mod 0x10"  # ActionRequest module +0x10: u64 held actions


def compile_script(text):
    """Named script -> raw op lines for play.rs."""
    out = []
    for n, line in enumerate(text.splitlines(), 1):
        w = line.split("#", 1)[0].split()
        if not w:
            continue
        f, op = int(w[0]), w[1]
        if op == "stick":
            out.append(f"{f} stick {float(w[2])} {float(w[3])}")
        elif op in ("press", "release", "tap"):
            if w[2] not in ACTIONS:
                raise ValueError(f"line {n}: unknown action {w[2]!r} (known: {', '.join(ACTIONS)})")
            b = ACTIONS[w[2]]
            if op == "tap":
                out += [f"{f} bit {HELD} {b} 1", f"{f + int(w[3]) if len(w) > 3 else f + 2} bit {HELD} {b} 0"]
            else:
                out.append(f"{f} bit {HELD} {b} {1 if op == 'press' else 0}")
        elif op == "free":
            out += [f"{f} free mod", f"{f} free pad", f"{f} free stick"]
        elif op == "end":
            out.append(f"{f} end")
        else:
            raise ValueError(f"line {n}: unknown op {op!r}")
    return "\n".join(out) + "\n"


def play(rec, script):
    """Compile `script` into <rec>/play_<name>.txt and ask the running recorder (inbox) to play it."""
    src = Path(script)
    if not src.exists():
        src = Path(__file__).parent / "inputs" / (script if script.endswith(".txt") else script + ".txt")
    raw = Path(rec) / f"play_{src.stem}.txt"
    raw.write_text(compile_script(src.read_text(encoding="utf-8")), encoding="utf-8")
    with open(Path(rec) / "inbox.txt", "a", encoding="utf-8") as fh:
        fh.write(f"mark play {src.stem}\nplay {raw.resolve()}\n")
    return raw
