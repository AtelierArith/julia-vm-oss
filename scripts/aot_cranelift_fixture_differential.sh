#!/usr/bin/env bash
# aot_cranelift_fixture_differential.sh
#
# Compare stdout from the same Julia fixture across:
#   1. upstream Julia
#   2. the Rust AoT backend generated binary
#   3. the Cranelift in-process JIT backend
#
# Usage:
#   bash scripts/aot_cranelift_fixture_differential.sh path/to/fixture.jl [...]
#
# Requirements:
#   - julia on PATH
#   - $CARGO_TARGET_DIR/release/juliars already built with Cranelift support
#     (or JULIARS_BIN set):
#       cargo build --release -p subset_julia_vm --features cranelift --bin juliars
#
# This is intentionally a developer helper, not a `check_*.sh` CI audit script.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "$ROOT/scripts/cargo_target_dir.sh"
cargo_target_dir="$(resolve_cargo_target_dir "$ROOT")"
export CARGO_TARGET_DIR="$cargo_target_dir"
JULIARS_BIN="${JULIARS_BIN:-$cargo_target_dir/release/juliars}"
export JULIARS_BIN

if [[ $# -lt 1 ]]; then
    echo "Usage: bash scripts/aot_cranelift_fixture_differential.sh <fixture.jl> [...]" >&2
    exit 2
fi

if ! command -v julia >/dev/null 2>&1; then
    echo "ERROR: 'julia' is not on PATH. Install upstream Julia or skip this differential check." >&2
    exit 2
fi

if [[ ! -x "$JULIARS_BIN" ]]; then
    echo "ERROR: juliars binary not built. Run:" >&2
    echo "  cargo build --release -p subset_julia_vm --features cranelift --bin juliars" >&2
    exit 2
fi

if ! "$JULIARS_BIN" --help | grep -q -- "--jit-run"; then
    echo "ERROR: juliars does not expose --jit-run. Rebuild with current sources and the cranelift feature." >&2
    exit 2
fi

tmp_root="$(mktemp -d "${TMPDIR:-/tmp}/sjulia-aot-cranelift-diff.XXXXXX")"
cleanup() {
    rm -rf "$tmp_root"
}
trap cleanup EXIT

for fixture in "$@"; do
    if [[ ! -f "$fixture" ]]; then
        echo "ERROR: fixture not found: $fixture" >&2
        exit 2
    fi

    tmp_dir="$tmp_root/$(basename "$fixture").d"
    mkdir -p "$tmp_dir"

    generated_rs="$tmp_dir/generated.rs"
    rust_bin="$tmp_dir/rust_backend_bin"
    rust_out="$tmp_dir/rust.out"
    cranelift_out="$tmp_dir/cranelift.out"
    julia_out="$tmp_dir/julia.out"

    if ! timeout 1800 "$JULIARS_BIN" "$fixture" -o "$generated_rs" --emit-binary "$rust_bin" >"$tmp_dir/juliars-rust.out" 2>&1; then
        echo "ERROR: Rust backend juliars failed for $fixture" >&2
        tail -40 "$tmp_dir/juliars-rust.out" >&2
        exit 1
    fi

    if ! timeout 120 "$rust_bin" >"$rust_out" 2>&1; then
        echo "ERROR: Rust backend generated binary failed for $fixture" >&2
        tail -40 "$rust_out" >&2
        exit 1
    fi

    if ! timeout 120 "$JULIARS_BIN" "$fixture" --backend cranelift --jit-run >"$cranelift_out" 2>&1; then
        echo "ERROR: Cranelift JIT run failed for $fixture" >&2
        tail -40 "$cranelift_out" >&2
        exit 1
    fi

    if ! timeout 120 julia "$fixture" >"$julia_out" 2>&1; then
        echo "ERROR: upstream julia failed for $fixture" >&2
        tail -40 "$julia_out" >&2
        exit 1
    fi

    if ! diff -u "$julia_out" "$rust_out"; then
        echo "MISMATCH: Rust backend stdout differs from upstream julia for $fixture" >&2
        exit 1
    fi

    if ! diff -u "$julia_out" "$cranelift_out"; then
        echo "MISMATCH: Cranelift JIT stdout differs from upstream julia for $fixture" >&2
        exit 1
    fi

    echo "OK: $fixture stdout matches across upstream julia, Rust backend, and Cranelift JIT."
done
