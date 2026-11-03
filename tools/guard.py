"""Repository guard: keeps game data, decompiled code and other forbidden content out of git.

Hooks (installed with `git config core.hooksPath .githooks`):
  pre-commit   python tools/guard.py staged
  commit-msg   python tools/guard.py message <file>
  pre-push     python tools/guard.py push          (what the push adds over origin/main, or upstream/main in a fork)
Other modes:
  python tools/guard.py diff <base>   files and commit messages between <base> and HEAD (CI on a pull request)
  python tools/guard.py tree          every tracked file
  python tools/guard.py selftest

Refused: game folders (extracted/, snapshots, decomp output), the game's file types (.dcx .bnd .hkx .tae .flver
.fsb .bin ...), binary files outside docs/media, big files, emulator case JSON, hex dumps, disassembly listings,
decompiled C, and blocked words.

Blocked words are never written in the repository: one per line in `.git/info/blocked-words` (local, untracked;
lines starting with # are comments), or in the environment variable GUARD_BLOCKED_WORDS (one per line or comma
separated; CI fills it from a secret). Matching ignores case.
"""

import os
import re
import subprocess
import sys

MAX_TEXT = 2 << 20     # mechanics-atlas.json is about 1.3 MB
MAX_MEDIA = 10 << 20   # docs/media/preview.webp is about 6.7 MB
MEDIA_DIR = "docs/media/"
MEDIA_EXT = (".png", ".jpg", ".jpeg", ".gif", ".webp")

# Folders that only ever hold game data or build output (any depth).
BLOCKED_DIRS = ("extracted", "image", "ghidra", "work", "target", "dist", "snapshot", "decomp")
# The game's file types, archives and executables, saves, Ghidra files.
BLOCKED_EXT = (
    ".dcx", ".bnd", ".bdt", ".bhd", ".bhf", ".bdf", ".hkx", ".tae", ".flver", ".fsb", ".fev", ".bank", ".bin",
    ".tpf", ".dds", ".hks", ".luabnd", ".emevd", ".msb", ".nvm", ".fmg", ".param", ".gparam", ".mtd", ".fxr",
    ".anibnd", ".chrbnd", ".behbnd", ".texbnd", ".partsbnd", ".parambnd", ".msgbnd", ".sl2",
    ".exe", ".dll", ".pdb", ".obj", ".lib", ".gzf", ".rep",
)
# Magic numbers of the game's containers, so a renamed file is caught too.
MAGIC = (b"DCX\0", b"BND3", b"BND4", b"BHF3", b"BHF4", b"BDF3", b"BDF4", b"TAE ", b"FLVER\0", b"FSB5", b"TPF\0",
         b"MZ")

HEX_RUN = re.compile(r"(?:\b[0-9A-Fa-f]{2}\b[ ,]+){48,}|[0-9A-Fa-f]{256,}")
MNEMONIC = (r"(?:mov|movzx|movsx|movss|movsd|movaps|movups|lea|call|jmp|push|pop|sub|add|imul|cmp|test|ret|xor|and|or"
            r"|shl|shr|sar|je|jne|jz|jnz|ja|jb|jg|jl|jae|jbe|jge|jle|cmov\w+|mulss|addss|subss|divss|comiss|ucomiss"
            r"|cvtss2sd|cvttss2si|nop|int3)")
# An address (and maybe the code bytes) before a mnemonic: Ghidra listings, objdump, capstone.
OPERANDS = (r"\b(?:\s*$|[^\n]*?(?:,|\[|\b(?:[re]?[abcd]x|[re]?[sd]i|[re]?[sb]p|r\d+[dwb]?|xmm\d+|0x[0-9a-f]+"
            r"|[0-9a-f]{6,}h?)\b))")
ASM_LINE = re.compile(r"^\s*(?:0x)?[0-9A-Fa-f]{6,16}:?\s+(?:[0-9A-Fa-f]{2}\s)*\s*" + MNEMONIC + OPERANDS, re.I)
# A bare instruction with register or memory operands.
ASM_BARE = re.compile(r"^\s*" + MNEMONIC + r"\s+(?:[re]?[abcd]x|[re]?[sd]i|[re]?[sb]p|r\d+[dwb]?|xmm\d+"
                      r"|(?:byte|word|dword|qword|xmmword) ptr)\b", re.I)
# Words only decompilers write (Ghidra, IDA), on a line that ends like a C statement.
DECOMP_WORD = re.compile(
    r"\b(?:undefined[1248]|undefined \*|u?longlong|CONCAT\d\d|SUB\d\d|ZEXT\d\d|SEXT\d\d|extraout_\w+|in_[A-Z]{2,3}|unaff_\w+"
    r"|_QWORD|_DWORD|_WORD|_BYTE|__int64|__int32|LODWORD|HIDWORD|param_\d+|local_[0-9a-f]+|[a-z]{1,3}Var\d+)\b"
    r"|\*\((?:undefined\d?|u?longlong|float|int|uint|ushort|byte|char|code|double) \*+\)")
# Emulator case files (tools/emu-cases): numbers computed from the game's memory; they stay in extracted/emu.
EMU_JSON = re.compile(r'"function"\s*:\s*"0x14[0-9A-Fa-f]{7}')


def git(*args):
    return subprocess.run(["git", *args], capture_output=True, check=True).stdout


def blocked_words():
    words = []
    try:
        common = git("rev-parse", "--git-common-dir").decode().strip()
        with open(os.path.join(common, "info", "blocked-words"), encoding="utf-8") as f:
            words += [w.strip() for w in f]
    except (OSError, subprocess.CalledProcessError):
        pass
    words += re.split(r"[\n,]", os.environ.get("GUARD_BLOCKED_WORDS", ""))
    return [w.lower() for w in words if w.strip() and not w.strip().startswith("#")]


WORDS = None


def words():
    global WORDS
    if WORDS is None:
        WORDS = blocked_words()
    return WORDS


def blobs(rev, names):
    """(name, bytes) for each `<rev>:<name>` through one `git cat-file --batch`."""
    names = [n for n in names if n]
    if not names:
        return []
    req = "".join(f"{rev}:{n}\n" for n in names).encode()
    out = subprocess.run(["git", "cat-file", "--batch"], input=req, capture_output=True, check=True).stdout
    res, pos = [], 0
    for n in names:
        end = out.index(b"\n", pos)
        head = out[pos:end].split()
        pos = end + 1
        if len(head) < 3 or head[1] == b"missing":
            continue
        size = int(head[2])
        res.append((n, out[pos:pos + size]))
        pos += size + 1
    return res


def as_text(data):
    """The file as text, or None for a binary file (UTF-16 with a byte order mark counts as text)."""
    if data[:2] in (b"\xff\xfe", b"\xfe\xff"):
        try:
            return data.decode("utf-16")
        except UnicodeDecodeError:
            return None
    nuls = data[:8192].count(0)
    if nuls == 0:
        return data.decode("utf-8", "replace")
    try:
        text = data.decode("utf-8")
    except UnicodeDecodeError:
        return None
    return text if nuls <= 4 else None


def problems(path, data):
    out = []
    p = path.replace("\\", "/")
    low_p = p.lower()
    dirs = low_p.split("/")[:-1]
    hit = [d for d in dirs if d in BLOCKED_DIRS]
    if hit:
        out.append(f"game data or build folder '{hit[0]}/'")
    if low_p.endswith(BLOCKED_EXT):
        out.append("game file type")
    if os.path.basename(low_p) in ("regions.json", "memory.bin"):
        out.append("emulator snapshot file")
    media = low_p.startswith(MEDIA_DIR) and low_p.endswith(MEDIA_EXT)
    if data.startswith(MAGIC) and not media:
        out.append("game container or executable (by its magic number)")
    if len(data) > (MAX_MEDIA if media else MAX_TEXT):
        out.append(f"too big ({len(data)} bytes)")
    text = as_text(data)
    if text is None:
        if not media:
            out.append("binary file (only images under docs/media/ may be binary)")
        return out
    low = text.lower()
    for w in words():
        if w in low:
            out.append("blocked word")
            break
    if HEX_RUN.search(text):
        out.append("long hex dump")
    lines = text.splitlines()
    if sum(1 for line in lines if ASM_LINE.match(line) or ASM_BARE.match(line)) >= 8:
        out.append("disassembly listing")
    if sum(1 for line in lines if line.rstrip().endswith((";", "{")) and DECOMP_WORD.search(line)) >= 6:
        out.append("decompiled C code")
    if low_p.endswith(".json") and EMU_JSON.search(text[:4096]):
        out.append("emulator case JSON (keep it in extracted/emu)")
    return out


def check_message(text):
    low = text.lower()
    return ["blocked word in commit message" for w in words() if w in low][:1]


def report(found, checked):
    for where, why in found:
        print(f"guard: {where}: {why}", file=sys.stderr)
    if found:
        print("guard: refused. Game data, decompiled code, disassembly and blocked words never go into this "
              "repository (docs/CONTRIBUTING.md).", file=sys.stderr)
        return 1
    print(f"guard: {checked} file(s) clean")
    return 0


def check_files(rev, names):
    found = []
    items = blobs(rev, names)
    for n, data in items:
        found += [(n, w) for w in problems(n, data)]
    return found, len(items)


def check_range(base):
    """Files changed and commit messages added between base and HEAD; every file when base is None."""
    if base:
        names = git("diff", "--name-only", "--diff-filter=ACMR", "-z", base, "HEAD").decode().split("\0")
    else:
        names = git("ls-tree", "-r", "--name-only", "-z", "HEAD").decode().split("\0")
    found, n = check_files("HEAD", names)
    if base:
        for msg in git("log", "--format=%B%x00", f"{base}..HEAD").decode("utf-8", "replace").split("\0"):
            found += [("commit message", w) for w in check_message(msg)]
    return found, n


def merge_base(ref):
    try:
        return git("merge-base", ref, "HEAD").decode().strip()
    except subprocess.CalledProcessError:
        return None


def selftest():
    cases = {
        "src/ok.rs": (b"fn f() -> u32 { 1 }\n", False),
        "docs/kb/x.md": (b"FUN_140bdb470 computes the super armour hit.\n", False),
        "extracted/combat_data.json": (b"{}", True),
        "tools/x/snapshot/notes.txt": (b"x", True),
        "data/c0000.anibnd.dcx": (b"DCX\0", True),
        "renamed.txt": (b"BND4" + b"\1" * 8, True),
        "chr/c1000.tae": (b"TAE ", True),
        "docs/media/shot.png": (b"\x89PNG\r\n\x1a\n\0\0", False),
        "src/blob.dat": (b"\0\1\2\x80\xff" * 20, True),
        "src/magic.rs": (b'assert_eq!(&d[0..4], b"DCA\0");\n', False),
        "tools/defs/x.xml": ("<?xml version='1.0'?><a/>".encode("utf-16"), False),
        "x.txt": (("48 8b " * 60).encode(), True),
        "d.txt": ("\n".join(f"14000{i:04x}: mov rax, rcx" for i in range(10)).encode(), True),
        "e.txt": ("\n".join("movss xmm0, dword ptr [rcx+0x10]" for _ in range(10)).encode(), True),
        "f.md": ("\n".join(f"  uVar{i} = *(undefined8 *)(param_1 + 0x{i}0);" for i in range(8)).encode(), True),
        "out/case.json": (b'{"function": "0x140bdb470", "cases": []}', True),
        "crates/game/tests/data/m10_00_00_00.json": (b'{"parts": 12}', False),
    }
    global WORDS
    WORDS = ["secretword"]
    cases["w.md"] = (b"see SecretWord here", True)
    bad = 0
    for name, (data, should_fail) in cases.items():
        got = problems(name, data)
        ok = bool(got) == should_fail
        print(f"{'ok  ' if ok else 'FAIL'} {name} refused={bool(got)} {got}")
        bad += not ok
    if not check_message("Add SECRETWORD notes"):
        print("FAIL commit message word")
        bad += 1
    WORDS = None
    print("selftest passed" if not bad else f"selftest: {bad} failure(s)")
    return 1 if bad else 0


def main():
    mode = sys.argv[1] if len(sys.argv) > 1 else "staged"
    if mode == "selftest":
        return selftest()
    if mode == "staged":
        names = git("diff", "--cached", "--name-only", "--diff-filter=ACMR", "-z").decode().split("\0")
        found, n = check_files("", names)
    elif mode == "message":
        text = open(sys.argv[2], encoding="utf-8", errors="replace").read()
        return report([("message", w) for w in check_message(text)], 0)
    elif mode == "push":
        remotes = git("remote").decode().split()
        ref = "upstream/main" if "upstream" in remotes else "origin/main"
        found, n = check_range(merge_base(ref))
    elif mode == "diff":
        if len(sys.argv) < 3:
            sys.exit("usage: guard.py diff <base>")
        found, n = check_range(merge_base(sys.argv[2]))
    elif mode == "tree":
        found, n = check_range(None)
    else:
        sys.exit(__doc__)
    return report(found, n)


if __name__ == "__main__":
    sys.exit(main())
