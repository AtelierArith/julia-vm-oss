#!/usr/bin/env bash
# Install sjulia with embedded Base bytecode and prelude Program caches.
#
# Usage:
#   scripts/sjulia_install.sh
#   scripts/sjulia_install.sh --force-cache
#
# Can be run from any directory. Requires a Rust toolchain.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)" || exit 1
cd "$ROOT"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target}"

force_cache=false
for a; do
  case "$a" in
    --force-cache)
      force_cache=true
      ;;
    *)
      echo "ERROR: unknown argument: $a" >&2
      echo "Usage: $0 [--force-cache]" >&2
      exit 1
      ;;
  esac
done

SJULIA_BIN="$CARGO_TARGET_DIR/release/sjulia"
BASE_CACHE="$CARGO_TARGET_DIR/base_cache.bin"
PRELUDE_PROGRAM_CACHE="$CARGO_TARGET_DIR/prelude_program_cache.bin"
PRELUDE_DIR="$ROOT/subset_julia_vm/src/julia"

if ! command -v cargo >/dev/null 2>&1; then
  echo "ERROR: cargo not found. Install Rust: https://rustup.rs/" >&2
  exit 1
fi

echo "== [1/4] build host sjulia =="
# Do NOT set SJULIA_BASE_CACHE here — this build is what generates the caches.
cargo build --release -p subset_julia_vm --bin sjulia --features repl
[[ -x "$SJULIA_BIN" ]] || { echo "ERROR: missing sjulia binary: $SJULIA_BIN" >&2; exit 1; }

echo "== [2/4] generate prelude Program cache =="
mkdir -p "$CARGO_TARGET_DIR"
needs_regen=true
if [[ "$force_cache" == false && -f "$PRELUDE_PROGRAM_CACHE" && -x "$SJULIA_BIN" ]]; then
  newer=$(find "$PRELUDE_DIR" "$SJULIA_BIN" -newer "$PRELUDE_PROGRAM_CACHE" -print -quit 2>/dev/null || true)
  if [[ -z "$newer" ]]; then
    needs_regen=false
    echo "Prelude Program cache is up-to-date; skipping regeneration ($PRELUDE_PROGRAM_CACHE)"
  fi
fi
if [[ "$needs_regen" == true ]]; then
  "$SJULIA_BIN" --precompile-prelude "$PRELUDE_PROGRAM_CACHE"
fi
[[ -f "$PRELUDE_PROGRAM_CACHE" ]] || { echo "ERROR: missing prelude Program cache: $PRELUDE_PROGRAM_CACHE" >&2; exit 1; }

echo "== [3/4] generate Base bytecode cache =="
needs_regen=true
if [[ "$force_cache" == false && -f "$BASE_CACHE" && -x "$SJULIA_BIN" ]]; then
  newer=$(find "$PRELUDE_DIR" "$SJULIA_BIN" -newer "$BASE_CACHE" -print -quit 2>/dev/null || true)
  if [[ -z "$newer" ]]; then
    needs_regen=false
    echo "Base cache is up-to-date; skipping regeneration ($BASE_CACHE)"
  fi
fi
if [[ "$needs_regen" == true ]]; then
  "$SJULIA_BIN" --precompile-base "$BASE_CACHE"
fi
[[ -f "$BASE_CACHE" ]] || { echo "ERROR: missing Base cache: $BASE_CACHE" >&2; exit 1; }

echo "== [4/4] cargo install with embedded caches =="
SJULIA_BASE_CACHE="$BASE_CACHE" \
SJULIA_PRELUDE_PROGRAM_CACHE="$PRELUDE_PROGRAM_CACHE" \
  cargo install --force --bin sjulia --path subset_julia_vm --features repl

echo "== sjulia installed successfully =="
echo "Default binary location: ~/.cargo/bin/sjulia (override with CARGO_INSTALL_ROOT)"
