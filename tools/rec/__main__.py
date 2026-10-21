"""Sekiro live recordings -> small per-category summaries.  python -m tools.rec <command> ...

  new <name> [--note "..."] [--cats movement,combat]   make memscope-data/logs/rec_<time>_<name>/ + manifest
  keys <rec>                hotkey helper while recording (run in the background; ends with the recording)
                            F9 marker  F10 stop  F5 can't-die  F6 freeze AI  F7 speed 1 / 0.5 / 0.25
                            F3 save position  F4 teleport to saved position
  install                   copy tools/memscope/rec2.lua into the memscope scripts folder
  ingest <rec> [--force]    build <rec>/rec.db (done automatically by summary / query)
  summary <rec> --cat <c>   movement combat enemies bosses skills items camera sound effects script all  (<= 60 lines)
  query <rec> <preset> [args] [--limit N]   presets: see `query <rec> help`
  replay all [--cached]     every recording: the events that differ most (also in logs/replay_all.md)
  replay <rec> [--from T --to T]   our sim plays the recorded inputs; HKS events matched + anim timeline (tools/rec/replay.py)
  play <rec> <script>       scripted input while recording (tools/rec/inputs/<script>.txt; hook DLL with pad_play)
  compare <rec> --sim <sv1 trace csv>
  list                      rewrite docs/kb/recordings.md (one line per recording)

<rec> = a folder path or just its name (rec_20261015_...) under memscope-data/logs. "last" = newest.
"""
import json
import os
import shutil
import sys
import time
from pathlib import Path

# The folder holding this repo and its siblings (memscope-data, reference, ...).
WORKSPACE = Path(__file__).resolve().parents[3]
LOGS = Path(os.environ.get("REC_LOGS", WORKSPACE / "memscope-data" / "logs"))
SCRIPTS = Path(os.environ.get("REC_SCRIPTS", WORKSPACE / "memscope-data" / "scripts" / "sekiro.exe"))
REPO = Path(__file__).resolve().parents[2]


def resolve(rec):
    if rec == "last":
        recs = sorted((p for p in LOGS.glob("rec_*") if (p / "frames.bin").exists()), key=lambda p: p.stat().st_mtime)
        return recs[-1]
    p = Path(rec)
    return p if p.exists() else LOGS / rec


def opt(args, key, default=None):
    if key in args:
        i = args.index(key)
        v = args[i + 1]
        del args[i:i + 2]
        return v
    return default


def cmd_new(args):
    note = opt(args, "--note", "")
    cats = opt(args, "--cats", "")
    name = args[0] if args else "test"
    d = LOGS / f"rec_{time.strftime('%Y%m%d_%H%M%S')}_{name}"
    d.mkdir(parents=True)
    (d / "inbox.txt").write_text("")
    (d / "manifest.json").write_text(json.dumps({
        "name": name, "note": note, "cats": [c for c in cats.split(",") if c], "date": time.strftime("%Y-%m-%d %H:%M"),
        "game": "sekiro 1.06 (me3 sekiro-research.me3)", "recorder": "tools/memscope/rec2.lua"}, indent=1))
    print(d)


def cmd_keys(args):
    import ctypes
    import winsound
    d = resolve(args[0])
    user32 = ctypes.windll.user32
    VK = {"F3": 0x72, "F4": 0x73, "F5": 0x74, "F6": 0x75, "F7": 0x76, "F9": 0x78, "F10": 0x79}
    state = {"nodead": 0, "freezeai": 0, "speed": 0}
    speeds = [1.0, 0.5, 0.25]
    marks = 0
    down = {k: False for k in VK}
    inbox = open(d / "inbox.txt", "a", encoding="utf-8")

    def send(line, freq):
        inbox.write(line + "\n")
        inbox.flush()
        winsound.Beep(freq, 80)

    stop_at = None
    t_start = time.time()
    while True:
        for k, vk in VK.items():
            pressed = bool(user32.GetAsyncKeyState(vk) & 0x8000)
            if pressed and not down[k]:
                if k == "F9":
                    marks += 1
                    send(f"mark {marks}", 1200)
                elif k == "F10":
                    send("stop", 400)
                    stop_at = time.time()
                elif k == "F5":
                    state["nodead"] ^= 1
                    send(f"nodead {state['nodead']}", 900 if state["nodead"] else 500)
                elif k == "F6":
                    state["freezeai"] ^= 1
                    send(f"freezeai {state['freezeai']}", 900 if state["freezeai"] else 500)
                elif k == "F7":
                    state["speed"] = (state["speed"] + 1) % len(speeds)
                    send(f"speed {speeds[state['speed']]}", 700 + 200 * state["speed"])
                elif k == "F3":
                    send("savepos", 1000)
                elif k == "F4":
                    send("loadpos", 1100)
            down[k] = pressed
        ev = d / "events.jsonl"
        if ev.exists() and time.time() - t_start > 5:
            try:
                with open(ev, "rb") as fh:
                    fh.seek(max(0, ev.stat().st_size - 400))
                    if b'"kind":"end"' in fh.read():
                        break
            except OSError:
                pass
        if stop_at and time.time() - stop_at > 15:
            break
        if time.time() - t_start > 4 * 3600:
            break
        time.sleep(0.01)
    inbox.close()
    print(f"keys done, {marks} markers")


def cmd_install(_args):
    SCRIPTS.mkdir(parents=True, exist_ok=True)
    shutil.copy(REPO / "tools" / "memscope" / "rec2.lua", SCRIPTS / "rec2.lua")
    print(SCRIPTS / "rec2.lua")


def cmd_list(_args):
    from .summary import CATS
    lines = ["# Recordings of the real Sekiro (memscope)", "",
             "One line per recording: `python -m tools.rec list` rewrites this file. Read a summary with",
             "`python -m tools.rec summary <rec> --cat <movement|combat|enemies|bosses|skills|items|camera|sound|effects|script|all>`",
             "(cached in `<rec>/summary/<cat>.md`); drill down with `python -m tools.rec query <rec> help`.",
             "Folder: `..\\memscope-data\\logs`. Recorder: `tools/memscope/rec2.lua` (v1: recorder.lua).", "",
             "How to record (the user plays, the session records):",
             "1. Start Sekiro with the hook DLL: from `..\\me3` run `bin\\me3.exe launch -p sekiro-rec.me3`;",
             "   `me3\\sekiro-rec\\sv1_rec_hooks.log` lists the installed hooks.",
             "2. `python -m tools.rec install`, then `python -m tools.rec new <name> --note \"...\" --cats a,b` (prints the folder).",
             "3. In the background: `python -m tools.rec keys <rec>` (F9 marker, F10 stop, F5 can't-die, F6 freeze AI,",
             "   F7 speed 1/0.5/0.25, F3/F4 save/teleport position).",
             "4. memscope: attach sekiro.exe, scripts run rec2 with args {dir: <full folder path>, seconds: N}, timeout N + 60.",
             "5. `python -m tools.rec summary <rec> --cat every`, then `python -m tools.rec list` to refresh this file.",
             "Turn SekiroTool's infinite items / emblems off for item tests (the summaries warn when they were on).",
             "Our sim against a recording: `python -m tools.rec replay <rec> [--from T --to T]` plays its stick and buttons",
             "into the sim, with the art and prosthetic Wolf had (their resident SpEffects) and, while locked on, a",
             "stand-in at the lock target's place (the recorded enemy the camera looks at), and matches the W_ events",
             "both HKS scripts fired; enemy, swim, teleport and recorder-stall stretches are left out (until both sides",
             "play the same clip again, started within 2 ticks).", "",
             "| recording | date | what | categories | length | summaries |", "|---|---|---|---|---|---|"]
    for d in sorted(LOGS.glob("rec_*")):
        if not d.is_dir() or not (d / "frames.bin").exists():
            continue
        m = {}
        if (d / "manifest.json").exists():
            m = json.loads((d / "manifest.json").read_text())
        ml = {}
        if (d / "manifest_lua.json").exists():
            try:
                ml = json.loads((d / "manifest_lua.json").read_text())
            except json.JSONDecodeError:
                pass
        secs = ml.get("seconds")
        if secs is None and (d / "index.txt").exists():
            for line in (d / "index.txt").read_text().splitlines():
                if line.startswith("done:"):
                    secs = float(line.split()[4])
        date = m.get("date") or time.strftime("%Y-%m-%d", time.localtime((d / "frames.bin").stat().st_mtime))
        sums = sorted(p.stem for p in (d / "summary").glob("*.md")) if (d / "summary").exists() else []
        what = m.get("note") or ("v1 recorder" if not m else m.get("name", ""))
        lines.append(f"| {d.name} | {date} | {what} | {','.join(m.get('cats', []))} | {f'{secs:.0f} s' if secs else '-'} | {' '.join(sums) or '-'} |")
    out = REPO / "docs" / "kb" / "recordings.md"
    out.write_text("\n".join(lines) + "\n", encoding="utf-8")
    print(f"{out} ({len(lines) - 11} recordings)")


def main():
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    args = sys.argv[1:]
    if not args or args[0] in ("-h", "--help", "help"):
        print(__doc__)
        return
    cmd, args = args[0], args[1:]
    if cmd == "new":
        return cmd_new(args)
    if cmd == "keys":
        return cmd_keys(args)
    if cmd == "install":
        return cmd_install(args)
    if cmd == "list":
        return cmd_list(args)
    if cmd == "ingest":
        from .ingest import ingest
        force = "--force" in args
        args = [a for a in args if a != "--force"]
        print(ingest(resolve(args[0]), force=force))
        return
    if cmd == "summary":
        from .summary import summary, CATS
        cat = opt(args, "--cat", "all")
        rec = resolve(args[0])
        cats = list(CATS) if cat == "every" else cat.split(",")
        for c in cats:
            sys.stdout.write(summary(rec, c))
        return
    if cmd == "query":
        from .query import run
        limit = int(opt(args, "--limit", 40))
        for line in run(resolve(args[0]), args[1] if len(args) > 1 else "help", args[2:], limit):
            print(line)
        return
    if cmd == "replay" and args and args[0] == "all":
        from .replay import replay_all
        lines = replay_all(LOGS, int(opt(args, "--limit", 25)), "--cached" in args)
        (LOGS / "replay_all.md").write_text("\n".join(lines) + "\n", encoding="utf-8")
        for line in lines:
            print(line)
        return
    if cmd == "replay":
        from .replay import replay
        t0, t1 = opt(args, "--from"), opt(args, "--to")
        for line in replay(resolve(args[0]), int(t0) if t0 else None, int(t1) if t1 else None, int(opt(args, "--limit", 12))):
            print(line)
        return
    if cmd == "play":
        from .play import play
        print(play(resolve(args[0]), args[1]))
        return
    if cmd == "compare":
        from .compare import compare
        sim = opt(args, "--sim")
        for line in compare(resolve(args[0]), sim):
            print(line)
        return
    print(__doc__)


if __name__ == "__main__":
    main()
