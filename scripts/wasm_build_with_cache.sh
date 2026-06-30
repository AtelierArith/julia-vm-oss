#!/usr/bin/env bash
# Build subset_julia_vm_web with wasm-pack, embedding the precompiled Base
# bytecode cache (Issue #2929) and parsed/lowered prelude Program cache
# (Issue #6026) into the WASM binary.
#
# At runtime, `compile_base_functions()` (subset_julia_vm/src/compile/cache.rs)
# loads Base bytecode from this embedded `&'static [u8]` instead of compiling
# Base from source. `parse_and_lower()` also loads the embedded prelude Program
# instead of parsing/lowering the prelude source on first `run_from_source()`.
# Remaining first-run work includes parser/lowering setup for the user program,
# embedded Base cache deserialize/restore, and user program compilation. The Web
# Playground should still run an explicit warmup before enabling the first user
# execution.
#
# Tradeoff: the embedded cache adds the cache file's size to the resulting
# .wasm. In current builds this is tens of MB before wasm-opt. Network transfer
# cost vs. Base compile latency — pick per deployment.
#
# Usage:
#   scripts/wasm_build_with_cache.sh                                # default: --target web
#   scripts/wasm_build_with_cache.sh --target nodejs
#   scripts/wasm_build_with_cache.sh --target web --out-dir ./web/pkg
#   scripts/wasm_build_with_cache.sh --force-cache ...              # force step 2 regen
#
# Script-only flag (stripped before forwarding to wasm-pack):
#   --force-cache   Regenerate target/base_cache.bin and
#                   target/prelude_program_cache.bin even if they look fresh.
#                   Equivalent to deleting the file beforehand.
#
# Remaining arguments are forwarded to `wasm-pack build`. Three normalizations:
#   * If no `--target` is given, `--target web` is appended.
#   * If the caller specifies no build profile/mode flag (`--profile`, `--dev`,
#     `--debug`, `--release`, `--profiling`), `--profile web-release` is
#     appended. That is the workspace-root profile carrying the WASM size
#     optimization (`opt-level = "s"`, `lto = true`) — see root `Cargo.toml`
#     and Issue #6922. Without it, wasm-pack's default `release` build would
#     get the un-optimized default settings.
#   * Any relative `--out-dir <path>` (or `--out-dir=<path>`) is resolved
#     against the *invocation* PWD before being forwarded, so paths like
#     `./web/pkg` mean what the caller expects (the script `cd`s into the
#     web crate before invoking wasm-pack, which would otherwise re-root
#     the relative path against the crate directory).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target}"
BASE_CACHE="$CARGO_TARGET_DIR/base_cache.bin"
PRELUDE_PROGRAM_CACHE="$CARGO_TARGET_DIR/prelude_program_cache.bin"
SJULIA_BIN="$CARGO_TARGET_DIR/release/sjulia"
PRELUDE_DIR="$ROOT/subset_julia_vm/src/julia"
WEB_CRATE="$ROOT/subset_julia_vm_web"
ORIG_PWD="$(pwd)"

# Strip --force-cache out of $@ before the wasm-pack arg-rewriting loop below.
force_cache=false
filtered=()
for a in "$@"; do
  if [[ "$a" == "--force-cache" ]]; then
    force_cache=true
  else
    filtered+=("$a")
  fi
done
if [[ ${#filtered[@]} -gt 0 ]]; then
  set -- "${filtered[@]}"
else
  set --
fi

if ! command -v wasm-pack >/dev/null 2>&1; then
  echo "ERROR: wasm-pack not found. Install via: cargo install wasm-pack" >&2
  exit 1
fi

echo "== [1/4] build host sjulia =="
# IMPORTANT: do NOT set SJULIA_BASE_CACHE here — this build is what
# generates the caches. Setting it would make build.rs panic.
cargo build --release --bin sjulia --features repl

echo "== [2/4] generate prelude Program cache =="
mkdir -p "$CARGO_TARGET_DIR"
needs_regen=true
if [[ "$force_cache" == false && -f "$PRELUDE_PROGRAM_CACHE" && -f "$SJULIA_BIN" ]]; then
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
# Regenerate only when needed. subset_julia_vm/build.rs lists base_cache.bin as
# a build dependency (include_bytes!), so any rewrite updates the mtime and can
# invalidate the subset_julia_vm rlib, forcing a full WASM relink. The
# `web-release` profile (workspace-root Cargo.toml, Issue #6922) uses
# lto = true, which makes that relink the dominant cost on repeated runs.
#
# Skip regeneration when an existing cache is newer than both the sjulia
# binary that produced it and the prelude sources. Pass --force-cache (or
# delete the file) to override.
needs_regen=true
if [[ "$force_cache" == false && -f "$BASE_CACHE" && -f "$SJULIA_BIN" ]]; then
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

echo "== [4/4] wasm-pack build (embedded prelude/Base caches) =="
# Rewrite args:
#   * detect whether the caller passed --target (we default to web if not)
#   * detect whether the caller chose a build profile/mode (we default to
#     --profile web-release if not — see Issue #6922)
#   * resolve any relative --out-dir / --out-dir= against ORIG_PWD
has_target=false
has_profile=false
wasm_pack_args=()
while [[ $# -gt 0 ]]; do
  case "$1" in
    --target)
      has_target=true
      wasm_pack_args+=("$1")
      shift
      if [[ $# -gt 0 ]]; then
        wasm_pack_args+=("$1")
        shift
      fi
      ;;
    --target=*)
      has_target=true
      wasm_pack_args+=("$1")
      shift
      ;;
    --profile)
      has_profile=true
      wasm_pack_args+=("$1")
      shift
      if [[ $# -gt 0 ]]; then
        wasm_pack_args+=("$1")
        shift
      fi
      ;;
    --profile=* | --dev | --debug | --release | --profiling)
      has_profile=true
      wasm_pack_args+=("$1")
      shift
      ;;
    --out-dir)
      wasm_pack_args+=("$1")
      shift
      if [[ $# -gt 0 ]]; then
        if [[ "$1" != /* ]]; then
          wasm_pack_args+=("$ORIG_PWD/$1")
        else
          wasm_pack_args+=("$1")
        fi
        shift
      fi
      ;;
    --out-dir=*)
      val="${1#--out-dir=}"
      if [[ "$val" != /* ]]; then
        wasm_pack_args+=("--out-dir=$ORIG_PWD/$val")
      else
        wasm_pack_args+=("$1")
      fi
      shift
      ;;
    *)
      wasm_pack_args+=("$1")
      shift
      ;;
  esac
done

if [[ "$has_target" == false ]]; then
  wasm_pack_args+=(--target web)
fi
if [[ "$has_profile" == false ]]; then
  wasm_pack_args+=(--profile web-release)
fi

cd "$WEB_CRATE"
SJULIA_BASE_CACHE="$BASE_CACHE" \
SJULIA_PRELUDE_PROGRAM_CACHE="$PRELUDE_PROGRAM_CACHE" \
  exec wasm-pack build "${wasm_pack_args[@]}"
