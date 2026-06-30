#!/usr/bin/env bash
# aot_fixture_julia_parity.sh
#
# Build one Julia fixture through the AoT path (`juliars -> Rust -> cargo`) and
# compare the generated binary stdout with upstream Julia stdout.
#
# Usage:
#   bash scripts/aot_fixture_julia_parity.sh path/to/fixture.jl
#
# Requirements:
#   - julia on PATH
#   - ./target/release/juliars already built:
#       cargo build --release -p subset_julia_vm --features aot --bin juliars
#
# This is intentionally a developer helper, not a `check_*.sh` CI audit script.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

if [[ $# -ne 1 ]]; then
    echo "Usage: bash scripts/aot_fixture_julia_parity.sh <fixture.jl>" >&2
    exit 2
fi

fixture="$1"
if [[ ! -f "$fixture" ]]; then
    echo "ERROR: fixture not found: $fixture" >&2
    exit 2
fi

if ! command -v julia >/dev/null 2>&1; then
    echo "ERROR: 'julia' is not on PATH. Install upstream Julia or skip this parity check." >&2
    exit 2
fi

juliars_bin="$ROOT/target/release/juliars"
if [[ ! -x "$juliars_bin" ]]; then
    echo "ERROR: juliars binary not built. Run:" >&2
    echo "  cargo build --release -p subset_julia_vm --features aot --bin juliars" >&2
    exit 2
fi

tmp_dir="$(mktemp -d "${TMPDIR:-/tmp}/sjulia-aot-fixture-parity.XXXXXX")"
cleanup() {
    rm -rf "$tmp_dir"
}
trap cleanup EXIT

generated_rs="$tmp_dir/generated.rs"
aot_bin="$tmp_dir/fixture_bin"
aot_out="$tmp_dir/aot.out"
julia_out="$tmp_dir/julia.out"

if ! timeout 1800 "$juliars_bin" "$fixture" -o "$generated_rs" --emit-binary "$aot_bin" >"$tmp_dir/juliars.out" 2>&1; then
    echo "ERROR: juliars failed for $fixture" >&2
    tail -40 "$tmp_dir/juliars.out" >&2
    exit 1
fi

if ! timeout 120 "$aot_bin" >"$aot_out" 2>&1; then
    echo "ERROR: generated AoT binary failed for $fixture" >&2
    tail -40 "$aot_out" >&2
    exit 1
fi

if ! timeout 120 julia "$fixture" >"$julia_out" 2>&1; then
    echo "ERROR: upstream julia failed for $fixture" >&2
    tail -40 "$julia_out" >&2
    exit 1
fi

if ! diff -u "$julia_out" "$aot_out"; then
    echo "MISMATCH: AoT binary stdout differs from upstream julia for $fixture" >&2
    exit 1
fi

echo "OK: $fixture AoT stdout matches upstream julia."
