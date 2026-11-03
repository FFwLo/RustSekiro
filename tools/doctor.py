"""Check that this checkout is ready to work on RSekiro, and say how to fix what is missing.

Usage: python tools/doctor.py
Every line is OK, MISSING (needed) or OPTIONAL (only some rows need it). It ends with `ready` when nothing needed
is missing (exit code 0).
"""

import os
import re
import shutil
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
REPO = "FFwLo/RustSekiro"
DEFAULT_SEKIRO = r"C:\Program Files (x86)\Steam\steamapps\common\Sekiro"


def run(*cmd):
    try:
        r = subprocess.run(cmd, cwd=ROOT, capture_output=True, text=True, timeout=120)
        return r.returncode, (r.stdout + r.stderr).strip()
    except (OSError, subprocess.TimeoutExpired) as e:
        return 1, str(e)


def main_checkout():
    """The first worktree is the main checkout, where extracted/ lives (agent worktrees have none)."""
    code, out = run("git", "worktree", "list", "--porcelain")
    if code == 0 and out.startswith("worktree "):
        return os.path.normpath(out.splitlines()[0].split(" ", 1)[1])
    return ROOT


def data_root():
    """Where the tests read extracted/ and config.toml: SHINOBI_ROOT, else the main checkout."""
    return os.environ.get("SHINOBI_ROOT") or main_checkout()


def want_rust():
    text = open(os.path.join(ROOT, "Cargo.toml"), encoding="utf-8").read()
    m = re.search(r'rust-version\s*=\s*"(\d+)\.(\d+)', text)
    return (int(m.group(1)), int(m.group(2))) if m else (1, 0)


def rust_version():
    code, out = run("rustc", "--version")
    m = re.search(r"rustc (\d+)\.(\d+)", out) if code == 0 else None
    return (int(m.group(1)), int(m.group(2))) if m else None


def uses_sccache():
    cfg = os.path.join(ROOT, ".cargo", "config.toml")
    return os.path.exists(cfg) and "sccache" in open(cfg, encoding="utf-8").read()


def sekiro_dir(extracted):
    if os.environ.get("SEKIRO_DIR"):
        return os.environ["SEKIRO_DIR"]
    rec = os.path.join(extracted, "sekiro_dir.txt")
    if os.path.exists(rec):
        return open(rec, encoding="utf-8", errors="replace").read().strip()
    return DEFAULT_SEKIRO


def checks():
    yield "needed", "Python 3.10+", sys.version_info >= (3, 10), "install Python 3.10 or newer"
    want, have = want_rust(), rust_version()
    yield ("needed", f"Rust {want[0]}.{want[1]}+ (Cargo.toml rust-version)", bool(have) and have >= want,
           "install Rust from https://rustup.rs, then rustup update stable")
    if uses_sccache():
        yield ("needed", "sccache (.cargo/config.toml builds through it)", shutil.which("sccache") is not None,
               "cargo install sccache --locked, or set RUSTC_WRAPPER to an empty value for your builds")
    extracted = os.path.join(data_root(), "extracted")
    game = sekiro_dir(extracted)
    yield ("needed", "your own Sekiro install (patch 1.06)", os.path.exists(os.path.join(game, "sekiro.exe")),
           "install Sekiro from Steam, or set SEKIRO_DIR to its folder")
    yield ("needed", f"extracted data ({extracted})", os.path.exists(os.path.join(extracted, "combat_data.json")),
           "in the main checkout: powershell -ExecutionPolicy Bypass -File tools\\extract.ps1 "
           "(add -Sekiro \"<install folder>\"); docs/CONTRIBUTING.md 'Set up'")
    code, hooks = run("git", "config", "core.hooksPath")
    yield "needed", "guard hooks (core.hooksPath .githooks)", hooks == ".githooks", "git config core.hooksPath .githooks"
    code, _ = run(sys.executable, "-I", "tools/guard.py", "selftest")
    yield "needed", "guard self test", code == 0, "python tools/guard.py selftest shows what fails"
    code, _ = run("gh", "auth", "status")
    yield "needed", "gh logged in (claims and pull requests)", code == 0, "install GitHub CLI, then gh auth login"
    _, remotes = run("git", "remote")
    remote = "upstream" if "upstream" in remotes.split() else "origin"
    code, _ = run("git", "rev-parse", "--verify", "-q", f"{remote}/agents-work-board")
    yield "needed", f"work board fetched ({remote})", code == 0, f"git fetch {remote} agents-work-board"
    code, common = run("git", "rev-parse", "--git-common-dir")
    words = os.path.join(ROOT, common, "info", "blocked-words") if code == 0 else ""
    yield ("optional", "blocked-words list for the guard", bool(words) and os.path.exists(words),
           "ask the owner for the list and save it as .git/info/blocked-words (never commit it)")
    yield ("optional", "Node.js (the game's effect files in extract)", shutil.which("node") is not None,
           "install Node.js and run the extract again")


def main():
    missing = 0
    for kind, what, ok, fix in checks():
        tag = "OK      " if ok else ("MISSING " if kind == "needed" else "OPTIONAL")
        print(f"{tag} {what}" + ("" if ok else f"\n         fix: {fix}"))
        missing += (not ok) and kind == "needed"
    print("ready" if not missing else f"{missing} needed item(s) missing")
    return 1 if missing else 0


if __name__ == "__main__":
    sys.exit(main())
