#!/usr/bin/env bash
# Prints one exe function from the full Ghidra decompile (extracted/decomp), optionally only the
# lines matching a pattern with context.
#   tools/fn.sh 140b51680                 the whole function
#   tools/fn.sh 140b51680 0x70 3          lines matching 0x70 with 3 lines of context
#   tools/fn.sh callers 140b51680         functions that call it (file: address)
cd "$(dirname "$0")/.." || exit 1
if [ "$1" = "callers" ]; then
  addr=${2#0x}; addr=${addr#FUN_}
  grep -lF "FUN_$addr(" extracted/decomp/*.c | while read -r f; do
    awk -v pat="FUN_$addr(" -v self="$addr" '/^\/\/ @ /{fn=$3} index($0, pat) && fn != self {print FILENAME": "fn}' "$f"
  done | sort -u
  exit 0
fi
addr=${1#0x}; addr=${addr#FUN_}
file="extracted/decomp/${addr:0:5}.c"
[ -f "$file" ] || file=$(grep -l "^// @ $addr\$" extracted/decomp/*.c | head -1)
body=$(awk -v a="$addr" '$0 == "// @ " a {p=1} p{print} p&&/^}/{exit}' "$file")
[ -z "$body" ] && { echo "not found: $addr"; exit 1; }
if [ -n "$2" ]; then
  echo "$body" | grep -n -C "${3:-2}" -- "$2"
else
  echo "$body"
fi
