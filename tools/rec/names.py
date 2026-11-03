"""English row names for ids in summaries (labels only, never values): Smithbox's SDT param row names,
which come from the game's own text. Missing files -> ids print without names."""
import json
from functools import lru_cache
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3] / Path(r"reference\Smithbox-src\Smithbox.Release\Output\Assets\PARAM\SDT\Param Row Names\English")


@lru_cache(None)
def _table(param):
    p = ROOT / f"{param}.json"
    if not p.exists():
        return {}
    try:
        d = json.loads(p.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError):
        return {}
    return {e["ID"]: e["Entries"][0] for e in d.get("Entries", []) if e.get("Entries") and e["Entries"][0]}


def name(param, i, width=28):
    n = _table(param).get(i)
    return f"{i} {n[:width]}" if n else str(i)


def goods(i):
    return name("EquipParamGoods", i)


def speffect(i):
    return name("SpEffectParam", i, 24)


EMEDF = Path(__file__).resolve().parents[3] / Path(r"reference\DarkScript3\DarkScript3\Resources\sekiro-common.emedf.json")
# EMEDF arg types -> struct format (DarkScript3 EMEDF.cs: 0 u8, 1 u16, 2 u32, 3 s8, 4 s16, 5 s32, 6 f32, 8 u32)
_ARG = {0: "B", 1: "H", 2: "I", 3: "b", 4: "h", 5: "i", 6: "f", 8: "I"}


@lru_cache(None)
def _emedf():
    if not EMEDF.exists():
        return {}
    try:
        text = "\n".join(l for l in EMEDF.read_text(encoding="utf-8").splitlines() if not l.lstrip().startswith("//"))
        import re
        d = json.loads(re.sub(r",(\s*[\]}])", r"\1", text))  # the file has trailing commas
    except (OSError, json.JSONDecodeError):
        return {}
    return {(c["index"], i["index"]): (i["name"], [(a["name"], a["type"]) for a in i["args"]])
            for c in d.get("main_classes", []) for i in c.get("instrs", [])}


def emevd(bank, cmd, args_hex=None):
    """'2004:8 SetSpEffect(entity=1000, id=3000)' style label (names from DarkScript3, values from the game)."""
    import struct
    n, spec = _emedf().get((bank, cmd), (None, []))
    label = f"{bank}:{cmd}" + (f" {n}" if n else "")
    if not args_hex or not spec:
        return label
    b = bytes.fromhex(args_hex)
    out, off = [], 0
    for an, ty in spec:
        f = _ARG.get(ty)
        if f is None:
            break
        size = struct.calcsize(f)
        off = (off + size - 1) // size * size
        if off + size > len(b):
            break
        v = struct.unpack_from("<" + f, b, off)[0]
        off += size
        out.append(f"{an.split()[0].lower() if an else '?'}={v:g}" if isinstance(v, float) else f"{(an or '?').replace(' ', '')[:14]}={v}")
    return f"{label}({', '.join(out)})"
