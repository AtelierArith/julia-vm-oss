#!/usr/bin/env bash
# aot_vm_differential.sh
#
# Compare stdout from the sjulia VM and a generated AoT binary for one or more
# Julia source fixtures. This is a developer-side differential harness, not a
# CI `check_*.sh` audit.
#
# Usage:
#   bash scripts/aot_vm_differential.sh path/to/fixture.jl [...]
#
# Requirements:
#   cargo build --release -p subset_julia_vm --features aot --bin juliars
#   cargo build --release -p subset_julia_vm --features repl --bin sjulia

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "$ROOT/scripts/cargo_target_dir.sh"
cargo_target_dir="$(resolve_cargo_target_dir "$ROOT")"
export CARGO_TARGET_DIR="$cargo_target_dir"
JULIARS_BIN="${JULIARS_BIN:-$cargo_target_dir/release/juliars}"
SJULIA_BIN="${SJULIA_BIN:-$cargo_target_dir/release/sjulia}"
export JULIARS_BIN SJULIA_BIN

if [[ $# -lt 1 ]]; then
    echo "Usage: bash scripts/aot_vm_differential.sh <fixture.jl> [...]" >&2
    exit 2
fi

if [[ ! -x "$JULIARS_BIN" ]]; then
    echo "ERROR: juliars binary not built. Run:" >&2
    echo "  cargo build --release -p subset_julia_vm --features aot --bin juliars" >&2
    exit 2
fi

if [[ ! -x "$SJULIA_BIN" ]]; then
    echo "ERROR: sjulia binary not built. Run:" >&2
    echo "  cargo build --release -p subset_julia_vm --features repl --bin sjulia" >&2
    exit 2
fi

for fixture in "$@"; do
    if [[ ! -f "$fixture" ]]; then
        echo "ERROR: fixture not found: $fixture" >&2
        exit 2
    fi

    tmp_dir="$(mktemp -d "${TMPDIR:-/tmp}/sjulia-aot-vm-diff.XXXXXX")"
    cleanup() {
        rm -rf "$tmp_dir"
    }
    trap cleanup EXIT

    generated_rs="$tmp_dir/generated.rs"
    aot_bin="$tmp_dir/fixture_bin"
    aot_out="$tmp_dir/aot.out"
    vm_out="$tmp_dir/vm.out"

    if ! timeout 1800 "$JULIARS_BIN" "$fixture" -o "$generated_rs" --emit-binary "$aot_bin" >"$tmp_dir/juliars.out" 2>&1; then
        echo "ERROR: juliars failed for $fixture" >&2
        tail -40 "$tmp_dir/juliars.out" >&2
        exit 1
    fi

    if ! timeout 120 "$aot_bin" >"$aot_out" 2>&1; then
        echo "ERROR: generated AoT binary failed for $fixture" >&2
        tail -40 "$aot_out" >&2
        exit 1
    fi

    if ! timeout 120 "$SJULIA_BIN" "$fixture" >"$vm_out" 2>&1; then
        echo "ERROR: sjulia VM failed for $fixture" >&2
        tail -40 "$vm_out" >&2
        exit 1
    fi

    if ! diff -u "$vm_out" "$aot_out"; then
        echo "MISMATCH: AoT binary stdout differs from sjulia VM for $fixture" >&2
        exit 1
    fi

    echo "OK: $fixture AoT stdout matches sjulia VM."
    rm -rf "$tmp_dir"
    trap - EXIT
done
