"""Prosthetic tool models -> extracted/tool/ (read by src/model/tool.rs).

    python tools/tool_export.py [--sekiro "C:/.../Sekiro"] [--exe path/to/sekiro-extract.exe]

From the user's own install: /parts/wp_a_07xx.partsbnd (unpacked to extracted/parts/), per part
WP_A_07xx.flver (Model0), WP_A_07xx_1.flver (Model1) and, for 750 / 770, WP_A_07xx_2.flver
(Model2) with the part's .tpf:
  model_wp_a_07xx[_N].bin (+ .dummies.json), textures into extracted/tex,
  anim_wp_a_07xx[_N].bin from WP_A_07xx[_N].anibnd (the clips keyed by Wolf's anim: the
  "_N" suffix dropped),
and tools.json: EquipParamWeapon 70000-79999 -> equipModelId and WepAbsorpPosParam[absorpParamId]
left_0..left_5 / leftHang_0..5 (from extracted/json/params, written by tools/extract.ps1).
Output is game data: never commit or publish it.
"""
import argparse
import json
import os
import subprocess

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
EXTRACTED = os.path.join(ROOT, "extracted")


def find_exe(given):
    if given:
        return os.path.abspath(given)
    for p in ("tools/sekiro-extract/sekiro-extract.exe", "tools/sekiro-extract/target/release/sekiro-extract.exe"):
        p = os.path.join(ROOT, p)
        if os.path.exists(p):
            return p
    raise SystemExit("sekiro-extract.exe not found: build tools/sekiro-extract (cargo build --release)")


def run(exe, *args):
    # Rigid tool meshes: bone from NormalW, vertices moved out of bone space (flver.rs ref_pose).
    env = {**os.environ, "SEKIRO_FLVER_REF_POSE": "1"}
    r = subprocess.run([exe, *args], capture_output=True, text=True, env=env)
    out = (r.stdout + r.stderr).strip()
    if r.returncode != 0:
        print("FAILED", args[0], args[1] if len(args) > 1 else "", out[-400:])
    return r.returncode == 0


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--sekiro", default=r"C:\Program Files (x86)\Steam\steamapps\common\Sekiro")
    ap.add_argument("--exe")
    a = ap.parse_args()
    exe = find_exe(a.exe)
    # Unpack the 26 tool parts (BND4 expanded into <name>.partsbnd.d, anibnds inside too).
    run(exe, "unpack", a.sekiro, EXTRACTED, r"^/parts/wp_a_07[0-9]{2}\.")
    parts = os.path.join(EXTRACTED, "parts")
    out = os.path.join(EXTRACTED, "tool")
    tex = os.path.join(EXTRACTED, "tex")
    os.makedirs(out, exist_ok=True)
    n = 0
    for d in sorted(os.listdir(parts)):
        if not (d.startswith("wp_a_07") and d.endswith(".partsbnd.d")):
            continue
        stem = d[: -len(".partsbnd.d")]  # wp_a_0760
        up = stem.upper()
        src = os.path.join(parts, d)
        tpf = os.path.join(src, f"{up}.tpf")
        for suffix in ("", "_1", "_2", "_3"):
            flver = os.path.join(src, f"{up}{suffix}.flver")
            if not os.path.exists(flver):
                continue
            model = os.path.join(out, f"model_{stem}{suffix}.bin")
            run(exe, "model", flver, tpf if os.path.exists(tpf) else "-", model, tex)
            run(exe, "dummies", flver, os.path.join(out, f"model_{stem}{suffix}.dummies.json"))
            anibnd = os.path.join(src, f"{up}{suffix}.anibnd.d")
            if os.path.isdir(anibnd):
                run(exe, "anims", anibnd, os.path.join(out, f"anim_{stem}{suffix}.bin"), suffix)
            n += 1
    # Tool rows: model id and the left-weapon attach dummies.
    params = os.path.join(EXTRACTED, "json", "params")
    weapons = {r["id"]: r for r in json.load(open(os.path.join(params, "EquipParamWeapon.json"), encoding="utf-8"))["rows"]}
    absorp = {r["id"]: r for r in json.load(open(os.path.join(params, "WepAbsorpPosParam.json"), encoding="utf-8"))["rows"]}
    tools = {}
    for wid, r in sorted(weapons.items()):
        if not 70000 <= wid < 80000:
            continue
        ab = absorp.get(r.get("absorpParamId"), {})
        tools[str(wid)] = {
            "equipModelId": r.get("equipModelId"),
            "absorpParamId": r.get("absorpParamId"),
            **{f"left_{i}": ab.get(f"left_{i}", -1) for i in range(6)},
            **{f"leftHang_{i}": ab.get(f"leftHang_{i}", -1) for i in range(6)},
        }
    with open(os.path.join(out, "tools.json"), "w", encoding="utf-8", newline="\n") as f:
        json.dump(tools, f, indent=1)
    print(f"{n} tool models, {len(tools)} tool rows -> {out}")


if __name__ == "__main__":
    main()
