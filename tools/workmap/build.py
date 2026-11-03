"""Work map: one HTML page showing how far RSekiro is proven to match Sekiro and which board rows are open.

    python tools/workmap/build.py [--tests 813] [--out target/workmap.html] [--png]

--png also saves the top of the page as docs/media/workmap.png (the README's picture) with a headless Edge or
Chrome.

Reads docs/kb/coverage.md (run tools/coverage.py first to refresh it) and WORK.md from the board branch
(`upstream/agents-work-board` in a fork, else `origin/agents-work-board`; fetch first). Writes one
self-contained page from template.html. Names, ids and counts only: no game data goes into it.
"""
import argparse
import datetime
import json
import os
import pathlib
import re
import shutil
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(os.path.dirname(HERE))


def git(*args):
    return subprocess.run(["git", *args], cwd=REPO, capture_output=True, text=True, encoding="utf-8").stdout


def coverage():
    chars, systems, section = [], [], None
    with open(os.path.join(REPO, "docs", "kb", "coverage.md"), encoding="utf-8") as f:
        for line in f:
            if line.startswith("## "):
                section = line[3:].strip()
            if not line.startswith("|") or line.startswith("|---"):
                continue
            c = [x.strip() for x in line.strip().strip("|").split("|")]
            if section == "Characters" and re.match(r"c\d{4}$", c[0]):
                chars.append([c[0], re.sub(r"\s*\(.*?\)$", "", c[1])[:48], c[2], c[-1]])
            elif section == "Systems" and c[0] != "System":
                lines = sum(int(n) for n in re.findall(r"\((\d+)\)", c[1]))
                systems.append([c[0], lines, c[-1], c[6][:240]])
    return chars, systems


def board():
    remote = "upstream" if "upstream" in git("remote").split() else "origin"
    text = git("show", f"{remote}/agents-work-board:WORK.md")
    if not text:
        raise SystemExit(f"cannot read WORK.md from {remote}/agents-work-board: git fetch {remote} first")
    rows, kind = [], None
    for line in text.splitlines():
        m = re.match(r"## (\w+)", line)
        if m:
            kind = m.group(1)
        c = [x.strip() for x in line.strip().strip("|").split("|")]
        if line.startswith("| ") and len(c) >= 5 and re.match(r"[A-Z]\d+[a-z]?$", c[0]):
            rows.append([c[0], c[1], c[2], c[3], c[4], kind])
    held = claims(remote)
    for r in rows:
        if r[3] == "open" and r[0] in held:
            r[3] = held[r[0]]
    return rows


def claims(remote):
    """{row ID: "github/agent since date"} from the board's own board.py (reads the claims issue and open pull
    requests with gh); empty when gh is missing or not logged in."""
    src = git("show", f"{remote}/agents-work-board:board.py")
    try:
        r = subprocess.run([sys.executable, "-", "claims"], cwd=REPO, input=src, capture_output=True, text=True,
                           encoding="utf-8", timeout=120)
    except (OSError, subprocess.TimeoutExpired):
        return {}
    out = {}
    for line in r.stdout.splitlines() if r.returncode == 0 else []:
        m = re.match(r"(\S+)\s+(\S+) \(since ([\d-]+)\)", line)
        if m:
            out[m.group(1)] = f"{m.group(2)} since {m.group(3)}"
    return out


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--tests", type=int, default=0, help="passing tests in the full suite (default: count the #[test]s in src/)")
    ap.add_argument("--out", default=os.path.join(REPO, "target", "workmap.html"))
    ap.add_argument("--png", action="store_true", help="also write docs/media/workmap.png")
    a = ap.parse_args()
    chars, systems = coverage()
    if a.tests:
        tests, label = a.tests, "tests passing"
    else:  # CI has no game data to run the suite with: count the tests instead
        tests, label = len(re.findall(r"#\[test\]", git("grep", "-h", r"#\[test\]", "--", "src"))), "tests"
    data = {"chr": chars, "sys": systems, "rows": board(), "tests": tests, "testsLabel": label,
            "date": datetime.date.today().isoformat()}
    with open(os.path.join(HERE, "template.html"), encoding="utf-8") as f:
        page = f.read().replace("/*DATA*/null", json.dumps(data, ensure_ascii=False))
    os.makedirs(os.path.dirname(os.path.abspath(a.out)), exist_ok=True)
    with open(a.out, "w", encoding="utf-8", newline="\n") as f:
        f.write(page)
    print(f"{a.out}: {len(systems)} systems, {len(chars)} characters, {len(data['rows'])} rows")
    if a.png:
        picture(os.path.abspath(a.out))


def picture(page):
    """The header, totals, systems and bosses at 1200 px wide, in the browser's own theme."""
    browsers = [shutil.which(n) for n in ("msedge", "chrome", "google-chrome", "chromium")] + [
        r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
        r"C:\Program Files\Google\Chrome\Application\chrome.exe"]
    exe = next((b for b in browsers if b and os.path.exists(b)), None)
    if not exe:
        raise SystemExit("no Edge or Chrome found for --png")
    out = os.path.join(REPO, "docs", "media", "workmap.png")
    os.makedirs(os.path.dirname(out), exist_ok=True)
    subprocess.run([exe, "--headless=new", "--disable-gpu", "--hide-scrollbars", "--force-device-scale-factor=1",
                    "--window-size=1200,1328", "--virtual-time-budget=6000", f"--screenshot={out}",
                    pathlib.Path(page).as_uri()], capture_output=True, check=True)
    print(f"{out}: {os.path.getsize(out):,} bytes")


if __name__ == "__main__":
    main()
