"""One git worktree per agent; the game data stays in the main checkout.

  python tools/worktree.py add <agent> <row ID> <slug>   # ~/rsekiro-<agent> on work-<ID>-<slug>-<github>-<agent>
                                                         # (from upstream/main in a fork, else origin/main)
  python tools/worktree.py sync <agent>                  # copy the worktree's config.toml into its test root again
  python tools/worktree.py remove <agent>                # remove the worktree and its test root (branch kept)
  python tools/worktree.py list
  python tools/worktree.py board                         # ~/rsekiro-board on the board branch (owner, board edits)

A worktree has no `extracted/` (it is git-ignored). Its tests read the data through SHINOBI_ROOT, which must hold
`extracted/` and the branch's own `config.toml`. So `add` makes a test root next to the worktree
(~/rsekiro-<agent>-root) with a copy of the worktree's config.toml and a link to the main checkout's extracted/,
and prints the SHINOBI_ROOT line. `remove` takes the link away first (never following it), so the main checkout's
data can never be touched. Never link or copy extracted/ into the worktree itself.
"""

import os
import shutil
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


def git(*args, cwd=ROOT):
    return subprocess.run(["git", *args], cwd=cwd, check=True, capture_output=True, text=True).stdout.strip()


def main_checkout():
    """The first worktree is the main checkout, where extracted/ lives."""
    return os.path.normpath(git("worktree", "list", "--porcelain").splitlines()[0].split(" ", 1)[1])


def github_name():
    for cmd in (["gh", "api", "user", "--jq", ".login"], ["git", "config", "user.name"]):
        try:
            r = subprocess.run(cmd, capture_output=True, text=True)
        except OSError:
            continue
        if r.returncode == 0 and r.stdout.strip():
            return r.stdout.strip().replace(" ", "-")
    return "someone"


def remote():
    """`upstream` in a fork (the project's own repository), else `origin`."""
    return "upstream" if "upstream" in git("remote").split() else "origin"


def path_of(agent):
    return os.path.join(os.path.expanduser("~"), f"rsekiro-{agent}")


def root_of(agent):
    return path_of(agent) + "-root"


def is_link(p):
    if os.path.islink(p):
        return True
    return os.path.isdir(p) and os.path.realpath(p) != os.path.abspath(p)


def drop_link(p):
    """Remove a link to a folder without touching what it points to."""
    if not is_link(p):
        return
    if os.name == "nt":
        subprocess.run(["cmd", "/c", "rmdir", os.path.normpath(p)], check=True, capture_output=True)
    else:
        os.unlink(p)


def make_root(agent):
    """The test root: the worktree's config.toml (copied) and a link to the main checkout's extracted/."""
    root, data = os.path.normpath(root_of(agent)), os.path.normpath(os.path.join(main_checkout(), "extracted"))
    os.makedirs(root, exist_ok=True)
    shutil.copy2(os.path.join(path_of(agent), "config.toml"), os.path.join(root, "config.toml"))
    link = os.path.join(root, "extracted")
    if not os.path.exists(link):
        if not os.path.isdir(data):
            print(f"no {data} yet: run tools/extract.ps1 in the main checkout, then worktree.py sync {agent}")
            return
        if os.name == "nt":
            subprocess.run(["cmd", "/c", "mklink", "/J", link, data], check=True, capture_output=True)
        else:
            os.symlink(data, link, target_is_directory=True)
    print(f"test root {root}: config.toml from the worktree, extracted/ linked to {data}")
    print(f"run tests with: SHINOBI_ROOT={root} cargo test --profile quick")


def remove_root(agent):
    root = root_of(agent)
    if not os.path.isdir(root):
        return
    drop_link(os.path.join(root, "extracted"))
    if os.path.exists(os.path.join(root, "extracted")):
        sys.exit(f"{root}/extracted is not a link: remove it by hand after checking it")
    shutil.rmtree(root)


def main():
    need = {"add": 3, "sync": 1, "remove": 1, "list": 0, "board": 0}
    if len(sys.argv) < 2 or sys.argv[1] not in need or len(sys.argv) - 2 < need[sys.argv[1]]:
        sys.exit(__doc__)
    cmd, args = sys.argv[1], sys.argv[2:]
    if cmd == "list":
        print(git("worktree", "list"))
        return
    if cmd == "board":
        path = os.path.join(os.path.expanduser("~"), "rsekiro-board")
        if os.path.isdir(path):
            print(f"{path} exists: cd there and git pull --rebase")
            return
        r = remote()
        git("fetch", "-q", r, "agents-work-board")
        local = "agents-work-board" if r == "origin" else "board-local"
        git("worktree", "add", "-B", local, path, f"{r}/agents-work-board")
        git("branch", f"--set-upstream-to={r}/agents-work-board", local, cwd=path)
        print(f"{path} on {local} (tracks {r}/agents-work-board)")
        return
    agent = args[0]
    if cmd == "add":
        rid, slug = args[1], args[2]
        branch = f"work-{rid}-{slug}-{github_name()}-{agent}"
        r = remote()
        git("fetch", "-q", r, "main")
        git("worktree", "add", "-b", branch, path_of(agent), f"{r}/main")
        git("config", "core.hooksPath", ".githooks", cwd=path_of(agent))
        make_root(agent)
        print(f"{path_of(agent)} on branch {branch}")
    elif cmd == "sync":
        make_root(agent)
    elif cmd == "remove":
        remove_root(agent)
        git("worktree", "remove", path_of(agent))
        print(f"removed {path_of(agent)} and its test root (branch kept)")


if __name__ == "__main__":
    main()
