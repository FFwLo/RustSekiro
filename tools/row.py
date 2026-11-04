"""One board row from claim to pull request, in four commands.

  python tools/row.py                      # where you are: your claims, your pull requests, rows you can take
  python tools/row.py start <ID> [--agent <name>]
                                           # claim the row, make your worktree and test root, show what to read
  python tools/row.py check [<filter>]     # in your worktree: the guard, then the tests (all, or the filter);
                                           # a full run is remembered for submit
  python tools/row.py submit "<one line>" [--part] [--emu "<emu-cases command>" | --why "<reason>"] [--dry]
                                           # push to your fork and open the pull request, filled in
                                           # (--dry prints it and changes nothing; emu rows need --emu)
  python tools/row.py verify <PR number> [--snapshot <dir>] [--dry]
                                           # re-make an emu row's emulator cases from YOUR exe, run its tests on
                                           # them and say so on the pull request (it runs the PR's code: read
                                           # the diff first)
  python tools/row.py drop <ID>            # give the row back and remove your worktree

Your agent name (a short bird or animal word) is asked once and kept in git config `rsekiro.agent`.
A pull request that says `Board: <ID> done: ...` keeps your claim alive while it is open, and the row moves to
Done by itself when the owner merges it. Rules: AGENTS.md on the board branch.
"""
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
REPO = "FFwLo/RustSekiro"


def run(cmd, cwd=ROOT, check=True, **kw):
    r = subprocess.run(cmd, cwd=cwd, capture_output=True, text=True, encoding="utf-8", **kw)
    if check and r.returncode:
        sys.exit(f"{' '.join(cmd[:3])} failed: {(r.stderr or r.stdout).strip()}")
    return r.stdout.strip()


def git(*args, cwd=ROOT, check=True):
    return run(["git", *args], cwd=cwd, check=check)


def remote():
    return "upstream" if "upstream" in git("remote").split() else "origin"


def board(*args, cwd=ROOT):
    """Run board.py from the board branch (always the fresh copy)."""
    src = git("show", f"{remote()}/agents-work-board:board.py")
    r = subprocess.run([sys.executable, "-", *args], cwd=cwd, input=src, capture_output=True, text=True,
                       encoding="utf-8")
    return r.returncode, (r.stdout + r.stderr).strip()


def me():
    login = run(["gh", "api", "user", "--jq", ".login"], check=False)
    if not login:
        sys.exit("GitHub CLI is not logged in: run `gh auth login`")
    return login


def agent_name(given=None):
    if given:
        git("config", "--global", "rsekiro.agent", given)
        return given
    name = git("config", "rsekiro.agent", check=False)
    if not name:
        sys.exit("pick a short bird or animal word as your agent name, once: row.py start <ID> --agent wren")
    return name


def work_row(rid):
    text = git("show", f"{remote()}/agents-work-board:WORK.md")
    for line in text.splitlines():
        c = [x.strip() for x in line.strip().strip("|").split("|")]
        if line.startswith("| ") and c[0] == rid and len(c) >= 5:
            return c
    sys.exit(f"{rid} is not on the board (or it is done): row.py lists the open rows")


def here():
    """(worktree top, branch, row ID) of the current folder."""
    top = git("rev-parse", "--show-toplevel", cwd=os.getcwd())
    branch = git("rev-parse", "--abbrev-ref", "HEAD", cwd=top)
    m = re.match(r"work-([A-Z]\d+[a-z]?)-", branch)
    return top, branch, m.group(1) if m else None


def record_path(top):
    return os.path.join(git("rev-parse", "--absolute-git-dir", cwd=top), "rsekiro-check.json")


def status():
    login, r = me(), remote()
    git("fetch", "-q", r, "main", "agents-work-board")
    print(f"you: {login}/{git('config', 'rsekiro.agent', check=False) or '(no agent name yet)'}\n")
    print("your claims:")
    print("  " + (board("mine", login)[1].replace("\n", "\n  ") or "none"))
    prs = json.loads(run(["gh", "pr", "list", "--repo", REPO, "--author", login, "--state", "open", "--json",
                          "number,title,reviewDecision,statusCheckRollup,url"], check=False) or "[]")
    print("\nyour pull requests:")
    for p in prs:
        checks = {c.get("conclusion") or c.get("status") for c in p["statusCheckRollup"] or []}
        state = "failing" if "FAILURE" in checks else "running" if checks & {"IN_PROGRESS", "QUEUED", "PENDING"} \
            else "green" if checks else "no checks yet"
        print(f"  #{p['number']} {p['title']} (CI {state}; review {p['reviewDecision'] or 'waiting'}) {p['url']}")
    if not prs:
        print("  none")
    rows = board("open")[1].splitlines()
    print("\nopen rows (new here: a B row):")
    for line in [x for x in rows if x.startswith("B ")][:3] + [x for x in rows if not x.startswith("B ")][:5]:
        print("  " + line[:150])
    print("\nnext: python tools/row.py start <ID>")


def start(rid, given):
    agent, login, r = agent_name(given), me(), remote()
    git("fetch", "-q", r, "main", "agents-work-board")
    c = work_row(rid)
    path = os.path.join(os.path.expanduser("~"), f"rsekiro-{agent}")
    if os.path.isdir(path):
        sys.exit(f"{path} is still there (one row at a time per agent): finish it, or row.py drop <its ID>")
    code, out = board("claim", rid, agent)
    if code:
        sys.exit(out)
    print(out)
    words = re.findall(r"[a-z0-9]+", re.sub(r"`[^`]*`", " ", c[4].lower()))
    slug = "-".join(words[:4]) or "row"
    print(run([sys.executable, "-I", os.path.join("tools", "worktree.py"), "add", agent, rid, slug]))
    notes = sorted(set(re.findall(r"`([^`]+\.(?:md|rs|py|lua))`", c[4] + " " + (c[5] if len(c) > 5 else ""))))
    print(f"\n{rid}: {c[4]}")
    if notes:
        print("read first: " + ", ".join(notes))
    print(f"\nnext: cd {path}, work, then python tools/row.py check and python tools/row.py submit \"<one line>\"")


def check(filt):
    top, branch, rid = here()
    print(run([sys.executable, "-I", os.path.join(top, "tools", "guard.py"), "push"], cwd=top))
    test_root = top + "-root" if os.path.isdir(top + "-root") else os.environ.get("SHINOBI_ROOT", top)
    env = dict(os.environ, SHINOBI_ROOT=test_root)
    cmd = ["cargo", "test", "--profile", "quick"] + ([filt] if filt else [])
    print(f"SHINOBI_ROOT={test_root} {' '.join(cmd)}")
    p = subprocess.Popen(cmd, cwd=top, env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True,
                         encoding="utf-8", errors="replace")
    total = [0, 0, 0]
    for line in p.stdout:
        m = re.match(r"test result: \w+\. (\d+) passed; (\d+) failed; (\d+) ignored", line)
        if m:
            total = [a + int(b) for a, b in zip(total, m.groups())]
        elif "FAILED" in line or "panicked" in line or line.startswith("error"):
            print(line.rstrip())
    p.wait()
    result = f"test result: {total[0]} passed; {total[1]} failed; {total[2]} ignored"
    print(result)
    if p.returncode or total[1]:
        sys.exit("fix the failures before you submit")
    if filt:
        print("filtered run: run `row.py check` without a filter once before submit")
        return
    sha = git("rev-parse", "HEAD", cwd=top)
    with open(record_path(top), "w", encoding="utf-8") as f:
        json.dump({"sha": sha, "result": result, "data": os.path.isdir(os.path.join(test_root, "extracted"))}, f)
    print(f"remembered for submit (commit {sha[:7]})")


def proof(top, rid, emu, why):
    """An emu row's (C or E) value is trusted only when someone else can re-make it: the emulator command that made the cases,
    a case generator under tools/emu-cases/src/ and a `_matches_the_exe` test in the diff."""
    if work_row(rid)[1] != "emu" and not rid.startswith("E"):
        return ""
    if why:
        return f"Emulator: none ({why})"
    if not emu or not re.match(r"^\S*emu-cases(\.exe)?\s", emu.strip() + " "):
        sys.exit('an emu row needs --emu "emu-cases <sekiro.exe> <out dir> [--snapshot <dir>] <names>" '
                 '(the command that made your cases), or --why "<reason>" when it has no emulator check')
    files = git("diff", "--name-only", f"{remote()}/main...HEAD", cwd=top).splitlines()
    gen = [f for f in files if f.startswith("tools/emu-cases/src/") and not f.endswith("main.rs")]
    diff = git("diff", f"{remote()}/main...HEAD", "--", "src", "crates", cwd=top)
    if not gen or not re.search(r"^\+.*fn \w*_matches_the_exe", diff, re.M):
        sys.exit("an emu row's diff needs its case generator (tools/emu-cases/src/<topic>.rs) and a test named "
                 '`..._matches_the_exe`, so a reviewer can re-make the cases; else --why "<reason>"')
    return "Emulator: `" + re.sub(r"^\S*emu-cases(\.exe)?", "emu-cases", emu.strip()) + "`"


def submit(line, part, dry, emu=None, why=None):
    top, branch, rid = here()
    if not rid:
        sys.exit(f"branch {branch} is not a row branch (work-<ID>-...): start the row with row.py start")
    if git("status", "--porcelain", cwd=top):
        sys.exit("commit your changes first (git status shows uncommitted files)")
    try:
        rec = json.load(open(record_path(top), encoding="utf-8"))
    except OSError:
        rec = {}
    if rec.get("sha") != git("rev-parse", "HEAD", cwd=top):
        sys.exit("run `python tools/row.py check` on this commit first (the full suite, no filter)")
    login, r = me(), remote()
    base = f"{r}/main"
    git("fetch", "-q", r, "main")
    emu_line = proof(top, rid, emu, why)
    commits =git("log", "--format=- %s", f"{base}..HEAD", cwd=top)
    if not dry:
        git("push", "-q", "-u", "origin", f"HEAD:refs/heads/{branch}", cwd=top)
        exists = run(["gh", "pr", "list", "--repo", REPO, "--head", branch, "--json", "url", "--jq", ".[0].url"],
                     check=False)
        if exists:
            print(f"pushed; the pull request is updated: {exists}")
            return
    kind = "part" if part else "done"
    data = "with my own extracted/" if rec.get("data") else "WITHOUT extracted/ (data tests were skipped)"
    body = f"""## What changed

{line}

{commits}

## Board

Board: {rid} {kind}: {line}
{emu_line}

## How it was checked

- [x] `row.py check` at {rec['sha'][:7]}, full suite {data}: `{rec['result']}`
- [x] `python tools/guard.py push` passed
- [ ] New behaviour has a sim test; a unit test that reads game data carries `cfg_attr(not(shinobi_data), ignore)`
- [ ] For an emu row (C or E): the emulator run (functions, case count) and the kb / emulator-pilot.md notes; it counts as
      checked once someone else runs `python tools/row.py verify <this PR>` on their own copy
- [ ] The kb note says what is still not covered (`gap:`)

## Rules

- [x] No game files, extracted data, snapshots, emulator case JSON, hex dumps, disassembly or decompiled code (guard)
- [ ] The code and notes are in my own words; exe function addresses and offsets only
"""
    if dry:
        print(f"title: {rid}: {line}\n\n{body}\n(dry run: nothing pushed or opened)")
        return
    with tempfile.NamedTemporaryFile("w", suffix=".md", delete=False, encoding="utf-8") as f:
        f.write(body)
    head = branch if r == "origin" else f"{login}:{branch}"
    url = run(["gh", "pr", "create", "--repo", REPO, "--base", "main", "--head", head,
               "--title", f"{rid}: {line}", "--body-file", f.name], cwd=top)
    os.unlink(f.name)
    print(f"pull request: {url}\ntick the open boxes on it; the owner reviews and merges, and the board follows.")


def main_checkout():
    return os.path.dirname(os.path.normpath(git("rev-parse", "--path-format=absolute", "--git-common-dir")))


def exe_path():
    """Your own sekiro.exe: SEKIRO_EXE, else the main checkout's image/sekiro.exe, else the Steam install's."""
    steam = os.environ.get("SEKIRO_DIR", r"C:\Program Files (x86)\Steam\steamapps\common\Sekiro")
    for p in (os.environ.get("SEKIRO_EXE"), os.path.join(main_checkout(), "image", "sekiro.exe"),
              os.path.join(steam, "sekiro.exe")):
        if p and os.path.isfile(p):
            return p
    sys.exit("no sekiro.exe found: set SEKIRO_EXE to your copy")


def verify(n, snapshot, dry):
    pr = json.loads(run(["gh", "pr", "view", str(n), "--repo", REPO, "--json", "body,headRefOid,author"]))
    if pr["author"]["login"] == me():
        sys.exit("verify someone else's pull request: your own cases prove nothing new")
    m = re.search(r"^Emulator:\s*`emu-cases\s+(.+?)`", pr["body"] or "", re.M)
    if not m:
        sys.exit(f"#{n} has no Emulator: `emu-cases ...` line: nothing to re-make")
    words = m.group(1).split()
    if "--snapshot" in words:
        i = words.index("--snapshot")
        del words[i:i + 2]
        if not snapshot:
            sys.exit("its cases come from a snapshot of the game's memory: make your own "
                     "(emu-cases snapshot <pid> <dir>) and pass --snapshot <dir>")
    names = words[2:]  # after the PR's own <exe> <out dir>
    diff = run(["gh", "pr", "diff", str(n), "--repo", REPO])
    tests = sorted(set(re.findall(r"^\+.*fn (\w+_matches_the_exe)", diff, re.M)))
    if not tests:
        sys.exit(f"#{n} adds no `..._matches_the_exe` test: nothing to run on the cases")
    sha, exe = pr["headRefOid"], exe_path()
    path = os.path.join(os.path.expanduser("~"), f"rsekiro-verify-{n}")
    print(f"#{n} at {sha[:7]}: emu-cases {' '.join(names)}; tests {', '.join(tests)}")
    print(f"this builds and runs the pull request's code on your PC: read it first (gh pr diff {n} --repo {REPO})")
    if dry:
        print(f"(dry run: would check it out in {path}, make the cases from {exe}, run the tests, comment)")
        return
    git("fetch", "-q", remote(), f"pull/{n}/head")
    if os.path.isdir(path):
        git("worktree", "remove", "--force", path, check=False)
    git("worktree", "add", "-q", "--detach", path, sha)
    try:
        tool = os.path.join(path, "tools", "emu-cases")
        run(["cargo", "build", "--release", "--manifest-path", os.path.join(tool, "Cargo.toml")], cwd=path)
        built = os.path.join(tool, "target", "release")
        dll = os.path.join(main_checkout(), "tools", "emu-cases", "target", "release", "unicorn.dll")
        if not os.path.isfile(os.path.join(built, "unicorn.dll")) and os.path.isfile(dll):
            shutil.copy2(dll, built)
        out = os.path.join(path, "verify-cases")  # fresh: SHINOBI_EMU points the tests here, not at your own cases
        cmd = [os.path.join(built, "emu-cases.exe"), exe, out] + (["--snapshot", snapshot] if snapshot else []) + names
        r = subprocess.run(cmd, cwd=path, capture_output=True, text=True, encoding="utf-8", errors="replace")
        made = sorted(f for f in os.listdir(out) if f.endswith(".json")) if os.path.isdir(out) else []
        if r.returncode or not made:
            sys.exit(f"emu-cases made no cases (exit {r.returncode}): {(r.stderr or r.stdout)[-600:]}")
        print(f"made {len(made)} case files from your exe")
        env = dict(os.environ, SHINOBI_EMU=out, SHINOBI_ROOT=os.environ.get("SHINOBI_ROOT", main_checkout()))
        passed, failed = 0, []
        for t in tests:
            r = subprocess.run(["cargo", "test", "--profile", "quick", t], cwd=path, env=env, capture_output=True,
                               text=True, encoding="utf-8", errors="replace")
            ok = [int(x) for x in re.findall(r"test result: ok\. (\d+) passed", r.stdout)]
            if r.returncode or not sum(ok):
                failed.append(t)
            passed += sum(ok)
        what = f"FAILED: {', '.join(failed)}" if failed else f"passed on them ({passed} tests)"
        result = (f"verified on my copy at {sha[:7]}: `emu-cases {' '.join(names)}` made {len(made)} case files from "
                  f"my own exe{' and snapshot' if snapshot else ''}; {', '.join(tests)} {what}.")
        print(result)
        run(["gh", "pr", "comment", str(n), "--repo", REPO, "--body", result])
        print(f"commented on #{n}")
        if failed:
            sys.exit(1)
    finally:
        git("worktree", "remove", "--force", path, check=False)


def drop(rid):
    code, out = board("release", rid)
    print(out)
    agent = git("config", "rsekiro.agent", check=False)
    if agent and os.path.isdir(os.path.join(os.path.expanduser("~"), f"rsekiro-{agent}")):
        print(run([sys.executable, "-I", os.path.join("tools", "worktree.py"), "remove", agent], check=False))


def main():
    a = sys.argv[1:]
    if not a:
        return status()
    cmd = a[0]
    if cmd == "start" and len(a) >= 2:
        return start(a[1], a[a.index("--agent") + 1] if "--agent" in a else None)
    if cmd == "check":
        return check(a[1] if len(a) > 1 else None)
    def opt(k):
        return a[a.index(k) + 1] if k in a and a.index(k) + 1 < len(a) else None
    if cmd == "submit" and len(a) >= 2:
        return submit(a[1], "--part" in a, "--dry" in a, opt("--emu"), opt("--why"))
    if cmd == "verify" and len(a) >= 2:
        return verify(a[1], opt("--snapshot"), "--dry" in a)
    if cmd == "drop" and len(a) >= 2:
        return drop(a[1])
    sys.exit(__doc__)


if __name__ == "__main__":
    main()
