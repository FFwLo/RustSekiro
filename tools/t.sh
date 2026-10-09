#!/usr/bin/env bash
# Quiet test run: prints only failures, panics, compile errors and the totals.
#   tools/t.sh [test-name-filter]
cd "$(dirname "$0")/.." || exit 1
cargo test "$@" 2>&1 | grep -E "^error|^warning: unused|test result|FAILED|panicked|^---- " -A6 | grep -v "^--$" | head -60
