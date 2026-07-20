#!/usr/bin/env bash
# Issue #10008: display-text helpers must not route arbitrary values through
# binary `write(io, x)`. Use `print`/`show` for text paths and reserve `write`
# for string/char/byte/raw payloads.

set -euo pipefail

ERRORS=0
TMP_FILE="${TMPDIR:-/tmp}/sjulia-display-write-files.$$"
trap 'rm -f "$TMP_FILE"' EXIT

find subset_julia_vm/src/julia/base -maxdepth 1 -type f -name '*.jl' | sort > "$TMP_FILE"

while IFS= read -r file; do
  if ! awk -v file="$file" '
    /^[[:space:]]*#/ { next }
    {
      line = $0
      sub(/#.*/, "", line)
      if (line ~ /write[[:space:]]*\([[:space:]]*io[[:space:]]*,[[:space:]]*(x|arg|args|value|v|obj|item)[[:space:]]*[\),]/) {
        printf "%s:%d: display helper writes arbitrary arg through binary write; use print/show/String(...) for text paths (Issue #10008): %s\n", file, NR, $0
        found = 1
      }
    }
    END { exit found ? 1 : 0 }
  ' "$file"; then
    ERRORS=$((ERRORS + 1))
  fi
done < "$TMP_FILE"

if [[ "$ERRORS" -ne 0 ]]; then
  echo "FAILED: display-text helpers must not call write(io, x/arg/value) for arbitrary values (Issue #10008)." >&2
  exit 1
fi

echo "OK: Julia display-text helpers keep arbitrary values off binary write paths (Issue #10008)."
