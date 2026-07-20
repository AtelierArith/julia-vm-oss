#!/usr/bin/env bash
# Run cargo nextest with sjulia's compile, persistent Base, and persistent
# prelude caches disabled. This is the cold counterpart to test_with_cache.sh:
# it exercises source parse/lower/compile paths that embedded or persistent
# caches can otherwise skip.
#
# Usage:
#   scripts/test_without_cache.sh                           # full release suite
#   scripts/test_without_cache.sh --test fixture_tests      # subset
#   scripts/test_without_cache.sh --test fixture_tests array::
#
# All arguments are forwarded verbatim to `cargo nextest run --release`.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target}"

echo "== [1/2] remove persistent sjulia caches =="
rm -f "$CARGO_TARGET_DIR"/sjulia_base_cache_*.bin
rm -f "$CARGO_TARGET_DIR"/sjulia_prelude_program_*.bin
rm -rf "${TMPDIR:-/tmp}/subset_julia_vm_cache"

echo "== [2/2] cargo nextest run --release (cold/no sjulia caches) =="
unset SJULIA_BASE_CACHE
unset SJULIA_PRELUDE_PROGRAM_CACHE
export SUBSET_JULIA_VM_DISABLE_CACHE=1
export SUBSET_JULIA_VM_DISABLE_PERSISTENT_BASE_CACHE=1
export SUBSET_JULIA_VM_DISABLE_PERSISTENT_PRELUDE_CACHE=1
exec timeout 1800 cargo nextest run --release "$@"
