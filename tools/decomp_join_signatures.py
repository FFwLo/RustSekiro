"""Joins Ghidra signatures split over two lines (return type alone on one line, the name
on the next) in extracted/decomp/*.c so gamedb's single-line matcher indexes them."""
import glob, re, sys

root = sys.argv[1] if len(sys.argv) > 1 else "extracted/decomp"
name_line = re.compile(r"^[A-Za-z_][\w:<>~]*\s*\(")
total = 0
for path in glob.glob(f"{root}/*.c"):
    lines = open(path, encoding="utf-8", errors="replace").read().split("\n")
    out, i, n = [], 0, 0
    while i < len(lines):
        cur = lines[i]
        nxt = lines[i + 1] if i + 1 < len(lines) else ""
        # "undefined8 *" / "longlong" etc. at column 0, then "FUN_x(" at column 0 after a "// @" header.
        if cur and not cur.startswith((" ", "/", "{", "}", "#")) and "(" not in cur and ";" not in cur and name_line.match(nxt):
            out.append(cur.rstrip() + " " + nxt)
            i += 2
            n += 1
            continue
        out.append(cur)
        i += 1
    if n:
        open(path, "w", encoding="utf-8").write("\n".join(out))
        total += n
print(f"joined {total} split signatures")
