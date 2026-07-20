#!/usr/bin/env bash
# Run cargo nextest with the precompiled Base bytecode cache embedded into
# the test binaries. Every test process loads Base from a `static [u8]`
# instead of compiling from source or reading the persistent on-disk cache,
# which removes the first-process cold-start cost (~350 ms) and trims per-
# process load time (saves the file read).
#
# Usage:
#   scripts/test_with_cache.sh                           # prepare + run full suite
#   scripts/test_with_cache.sh --test fixture_tests      # prepare + run subset
#   scripts/test_with_cache.sh --prepare-only            # build/embed test artifacts
#   scripts/test_with_cache.sh --run-only --partition count:1/4
#
# All arguments are forwarded verbatim to `cargo nextest run --release`.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target}"
BASE_CACHE="$CARGO_TARGET_DIR/base_cache.bin"
PREPARE_ONLY=false
RUN_ONLY=false

while [[ $# -gt 0 ]]; do
  case "$1" in
    --prepare-only)
      PREPARE_ONLY=true
      shift
      ;;
    --run-only)
      RUN_ONLY=true
      shift
      ;;
    *)
      break
      ;;
  esac
done

if [[ "$PREPARE_ONLY" == true && "$RUN_ONLY" == true ]]; then
  echo "ERROR: --prepare-only and --run-only are mutually exclusive" >&2
  exit 2
fi

prepare_cache() {
  echo "== [1/3] build host sjulia =="
  # IMPORTANT: do NOT set SJULIA_BASE_CACHE here — this build is what
  # generates the cache. Setting it would make build.rs panic.
  cargo build --locked --release --bin sjulia --features repl

  echo "== [2/3] generate Base bytecode cache =="
  mkdir -p "$CARGO_TARGET_DIR"
  "$CARGO_TARGET_DIR/release/sjulia" --precompile-base "$BASE_CACHE"
  [[ -f "$BASE_CACHE" ]] || { echo "ERROR: missing Base cache: $BASE_CACHE" >&2; exit 1; }
}

prepare_nextest_binaries() {
  echo "== [3/3] cargo nextest run --release --no-run (embedded Base cache) =="
  SJULIA_BASE_CACHE="$BASE_CACHE" timeout 1800 cargo nextest run --locked --release --no-run "$@"
}

run_nextest() {
  [[ -f "$BASE_CACHE" ]] || { echo "ERROR: missing Base cache: $BASE_CACHE" >&2; exit 1; }
  echo "== cargo nextest run --release (embedded Base cache) =="
  SJULIA_BASE_CACHE="$BASE_CACHE" exec timeout 1800 cargo nextest run --locked --release "$@"
}

if [[ "$RUN_ONLY" != true ]]; then
  prepare_cache
fi

if [[ "$PREPARE_ONLY" == true ]]; then
  prepare_nextest_binaries "$@"
  exit 0
fi

run_nextest "$@"
