"""The combat core's links to the rest of the game: python tools/core_deps.py [-v]

The core is what another Bevy game would add to its own world (Wolf, enemies, combat, the HKS
scripts, animation, effects). Every `crate::<module>` use from a core module into a module
outside the core is a link still to cut; -v lists them per item.
"""
import collections
import os
import re
import sys

CORE = {
    "actor", "ai", "anim", "cloth", "combat", "config", "data", "docking", "emevd", "encounter", "enemy",
    "enemy_bullet", "ffx", "form", "fxr", "gore", "guard", "hkb_runtime", "hks", "hks_enemy",
    "hks_wolf", "interp", "items", "model", "navmesh", "ninjutsu", "paths", "player", "prosthetic", "seat", "sound_rules", "status", "stealth", "swim", "terrain", "traversal",
    "perf", "place", "vfx", "view", "wire",
}
# Test-only modules (#[cfg(test)] in lib.rs) are not links.
TEST_ONLY = {"hks_lua", "sim_tests", "shader_tests"}
SRC = os.path.join(os.path.dirname(__file__), "..", "src")


def module_files(m):
    out = [os.path.join(SRC, m + ".rs")] if os.path.exists(os.path.join(SRC, m + ".rs")) else []
    for root, _, files in os.walk(os.path.join(SRC, m)):
        out += [os.path.join(root, f) for f in files if f.endswith(".rs")]
    return out


def main():
    links = collections.Counter()
    for m in sorted(CORE):
        for f in module_files(m):
            text = open(f, encoding="utf-8").read()
            text = text.split("#[cfg(test)]\nmod tests")[0]
            for target, item in re.findall(r"crate::(\w+)(?:::(\w+))?", text):
                if target in TEST_ONLY:
                    continue
                if target not in CORE and os.path.exists(os.path.join(SRC, target + ".rs")) or os.path.isdir(os.path.join(SRC, target)) and target not in CORE:
                    links[(m, target, item or "*")] += 1
    by_target = collections.Counter()
    for (m, t, _), n in links.items():
        by_target[t] += n
    print(f"{sum(links.values())} links from the core to {len(by_target)} outside modules")
    for t, n in by_target.most_common():
        users = sorted({m for (m, tt, _) in links if tt == t})
        print(f"  {t:12} {n:3}  from {', '.join(users)}")
        if "-v" in sys.argv:
            items = collections.Counter()
            for (m, tt, i), k in links.items():
                if tt == t:
                    items[i] += k
            print("               " + ", ".join(f"{i} {k}" for i, k in items.most_common()))


if __name__ == "__main__":
    main()
