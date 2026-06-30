#!/usr/bin/env bash
# Check that bin/ crates do not use .expect() for user-facing errors.
# Binary crates must use unwrap_or_else(|e| { eprintln!(...); process::exit(1); }) instead.
# See: docs/vm/PANIC_FREE.md and Issue #3051.

set -euo pipefail

FOUND=$(grep -rn '\.expect(' subset_julia_vm/src/bin/ --include='*.rs' || true)
if [ -n "$FOUND" ]; then
  echo "ERROR: .expect() calls found in bin/ — use unwrap_or_else instead:"
  echo "$FOUND"
  exit 1
fi
echo "OK: No .expect() calls in bin/."
