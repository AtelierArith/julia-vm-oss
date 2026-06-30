#!/usr/bin/env bash
# Run cargo nextest with the precompiled Base bytecode cache embedded into
# the test binaries. Every test process loads Base from a `static [u8]`
# instead of compiling from source or reading the persistent on-disk cache,
# which removes the first-process cold-start cost (~350 ms) and trims per-
# process load time (saves the file read).
#
# Usage:
#   scripts/test_with_cache.sh                           # full release suite
#   scripts/test_with_cache.sh --test fixture_tests      # subset
#   scripts/test_with_cache.sh --test fixture_tests array::
#
# All arguments are forwarded verbatim to `cargo nextest run --release`.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target}"
BASE_CACHE="$CARGO_TARGET_DIR/base_cache.bin"

echo "== [1/3] build host sjulia =="
# IMPORTANT: do NOT set SJULIA_BASE_CACHE here — this build is what
# generates the cache. Setting it would make build.rs panic.
cargo build --release --bin sjulia --features repl

echo "== [2/3] generate Base bytecode cache =="
mkdir -p "$CARGO_TARGET_DIR"
"$CARGO_TARGET_DIR/release/sjulia" --precompile-base "$BASE_CACHE"
[[ -f "$BASE_CACHE" ]] || { echo "ERROR: missing Base cache: $BASE_CACHE" >&2; exit 1; }

echo "== [3/3] cargo nextest run --release (embedded Base cache) =="
SJULIA_BASE_CACHE="$BASE_CACHE" exec timeout 1800 cargo nextest run --release "$@"
