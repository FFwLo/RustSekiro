"""Read the work board and claim rows.

Read (from any checkout; in a fork say upstream where it says origin):
  git show origin/agents-work-board:board.py | python - open          # open rows; claimed rows are left out
  git show origin/agents-work-board:board.py | python - row <ID>
  git show origin/agents-work-board:board.py | python - claims        # live claims
  git show origin/agents-work-board:board.py | python - mine <github>
Claim (works from a fork; needs gh logged in):
  ... | python - claim <ID> <agent>     # comments "claim <ID> <agent>" on the "Work board claims" issue
  ... | python - release <ID>           # comments "release <ID>"
Owner, inside the board worktree on branch agents-work-board (then commit and push):
  ... | python - done <ID> "<one line result, PR>" <github>/<agent>

Claims are comments on one open issue titled "Work board claims". A comment's first line is
`claim <ID> <agent>` or `release <ID>`; the claim belongs to the comment's GitHub author, so nobody can claim in
someone else's name. The first live claim on a row wins. A claim lasts 7 days from its latest comment: comment the
same line again to keep it. A merged pull request moves the row to Done; `release` gives it back. The owner may
release any claim.
"""

import datetime
import json
import os
import subprocess
import sys

REPO = "FFwLo/RustSekiro"
OWNER = "FFwLo"
ISSUE_TITLE = "Work board claims"
CLAIM_DAYS = 7


def remote():
    """`upstream` in a fork (the project's own repository), else `origin`."""
    r = subprocess.run(["git", "remote"], capture_output=True, text=True)
    return "upstream" if "upstream" in r.stdout.split() else "origin"


BRANCH = f"{remote()}/agents-work-board"


def on_board():
    """Inside a board worktree (any branch name: a fork's copy is `board-local`)."""
    return all(os.path.exists(n) for n in ("WORK.md", "AGENTS.md", "board.py"))


def read(name):
    if on_board():
        return open(name, encoding="utf-8").read()
    r = subprocess.run(["git", "show", f"{BRANCH}:{name}"], capture_output=True, text=True, encoding="utf-8")
    if r.returncode:
        sys.exit(f"cannot read {name} from {BRANCH}: run `git fetch {BRANCH.split('/')[0]} agents-work-board` first")
    return r.stdout


def gh(*args):
    try:
        r = subprocess.run(["gh", *args], capture_output=True, text=True, encoding="utf-8", timeout=60)
    except (OSError, subprocess.TimeoutExpired) as e:
        sys.exit(f"gh failed ({e}): install GitHub CLI and run gh auth login")
    if r.returncode:
        sys.exit(f"gh failed: {r.stderr.strip()}")
    return r.stdout


def claims_issue():
    out = gh("issue", "list", "--repo", REPO, "--state", "open", "--search", f'"{ISSUE_TITLE}" in:title',
             "--json", "number,title")
    for i in json.loads(out or "[]"):
        if i["title"].strip() == ISSUE_TITLE:
            return i["number"]
    sys.exit(f'no open issue titled "{ISSUE_TITLE}" in {REPO}: ask the owner to open it')


def live_claims(done_ids):
    """{row ID: (github/agent, date)} from the claims issue's comments, oldest first."""
    n = claims_issue()
    out = gh("api", "--paginate", f"repos/{REPO}/issues/{n}/comments",
             "--jq", ".[] | {u: .user.login, t: .created_at, b: .body}")
    now = datetime.datetime.now(datetime.timezone.utc)
    live = {}
    for line in out.splitlines():
        c = json.loads(line)
        first = (c["b"] or "").strip().splitlines()[:1]
        words = first[0].split() if first else []
        if len(words) < 2:
            continue
        verb, rid, user = words[0].lower(), words[1].rstrip(":"), c["u"]
        when = datetime.datetime.fromisoformat(c["t"].replace("Z", "+00:00"))
        held = live.get(rid)
        if held and (when - held[2]).days >= CLAIM_DAYS:  # expired before this comment
            del live[rid]
            held = None
        if verb == "claim" and len(words) >= 3 and rid not in done_ids:
            if held is None or held[0] == user:
                live[rid] = (user, words[2], when)
        elif verb == "release" and held and (held[0] == user or user == OWNER):
            del live[rid]
    return {rid: (f"{u}/{a}", w.date().isoformat()) for rid, (u, a, w) in live.items()
            if (now - w).days < CLAIM_DAYS}


def cells(line):
    return [c.strip() for c in line.strip().strip("|").split("|")]


def rows(text):
    """(section, cells, line index) for every table row with an ID."""
    section, out = "", []
    for i, line in enumerate(text.splitlines()):
        if line.startswith("## "):
            section = line[3:].split(":")[0].strip()
        elif line.startswith("| ") and not line.startswith("| ID") and not line.startswith("|--"):
            c = cells(line)
            if len(c) >= 4:  # rows have 6 cells, Done rows 4
                out.append((section, c, i))
    return out


def find(text, rid, done=False):
    return next((r for r in rows(text) if r[1][0] == rid and (r[0] == "Done") == done), None)


def main():
    if len(sys.argv) < 2:
        sys.exit(__doc__)
    cmd, args = sys.argv[1], sys.argv[2:]
    work = read("WORK.md")
    done_ids = {c[0] for s, c, _ in rows(work) if s == "Done"}
    if cmd == "open":
        try:
            live = live_claims(done_ids)
        except SystemExit as e:
            print(f"warning: claims not read ({e}); check `board.py claims` before you claim", file=sys.stderr)
            live = {}
        for kind, c, _ in rows(work):
            if kind == "Done" or c[3] != "open" or c[1] == "humans" or c[0] in live:
                continue
            needs = [n.strip() for n in c[2].replace(",", " ").split() if n.strip()]
            wait = [n for n in needs if n not in done_ids]
            print(f"{kind:2} {c[0]:5} {c[1]:4} {c[4]}" + (f" (waits for {', '.join(wait)})" if wait else ""))
    elif cmd == "claims":
        for rid, (who, day) in sorted(live_claims(done_ids).items()):
            print(f"{rid:5} {who} (since {day})")
    elif cmd == "row":
        r, d = find(work, args[0]), find(work, args[0], done=True)
        if r:
            print(f"{r[0]}: " + " | ".join(r[1]))
            held = live_claims(done_ids).get(args[0])
            if held:
                print(f"claimed by {held[0]} since {held[1]}")
        if d:
            print("Done: " + " | ".join(d[1]))
        if not r and not d:
            print("no such row")
    elif cmd == "mine":
        for rid, (who, day) in sorted(live_claims(done_ids).items()):
            if who.split("/")[0] == args[0] or who == args[0]:
                print(f"{rid:5} {who} (since {day})")
        for kind, c, _ in rows(work):
            if kind != "Done" and args[0] in c[3]:
                print(f"{c[0]:5} {c[3]} (on the board)")
    elif cmd == "claim":
        if len(args) < 2:
            sys.exit("usage: claim <ID> <agent>")
        r = find(work, args[0])
        if not r:
            sys.exit("no such open row")
        if r[1][3] != "open" or r[1][1] == "humans":
            sys.exit(f"{args[0]} is not open: {r[1][3]} ({r[1][1]})")
        held = live_claims(done_ids).get(args[0])
        me = gh("api", "user", "--jq", ".login").strip()
        if held and held[0].split("/")[0] != me:
            sys.exit(f"{args[0]} is claimed by {held[0]} since {held[1]}: pick another row")
        gh("issue", "comment", str(claims_issue()), "--repo", REPO, "--body", f"claim {args[0]} {args[1]}")
        print(f"claimed {args[0]} for {me}/{args[1]} for {CLAIM_DAYS} days")
    elif cmd == "release":
        gh("issue", "comment", str(claims_issue()), "--repo", REPO, "--body", f"release {args[0]}")
        print(f"released {args[0]}")
    elif cmd == "done":
        if not on_board():
            sys.exit("run this inside the board worktree (branch agents-work-board)")
        if len(args) < 3:
            sys.exit('usage: done <ID> "<one line result, PR>" <github>/<agent>')
        r = find(work, args[0])
        if not r:
            sys.exit("no such open row in WORK.md")
        lines = work.splitlines()
        del lines[r[2]]
        today = datetime.date.today().isoformat()
        text = "\n".join(lines).rstrip("\n") + f"\n| {args[0]} | {args[2]} | {today} | {args[1]} |\n"
        open("WORK.md", "w", encoding="utf-8", newline="\n").write(text)
        print(f"done {args[0]}: WORK.md saved; add a line to CHANGES.md, commit and push the board")
    else:
        sys.exit(__doc__)


if __name__ == "__main__":
    main()
