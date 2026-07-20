#!/usr/bin/env bash
# aot_cranelift_backend_benchmark.sh
#
# Benchmark the same Julia fixture through the Rust AoT backend and the
# Cranelift in-process JIT backend.
#
# Usage:
#   ITERATIONS=5 bash scripts/aot_cranelift_backend_benchmark.sh path/to/fixture.jl [...]
#
# Requirements:
#   - perl with Time::HiRes (available on the supported developer platforms)
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
ITERATIONS="${ITERATIONS:-3}"

if [[ $# -lt 1 ]]; then
    echo "Usage: ITERATIONS=5 bash scripts/aot_cranelift_backend_benchmark.sh <fixture.jl> [...]" >&2
    exit 2
fi

if ! [[ "$ITERATIONS" =~ ^[1-9][0-9]*$ ]]; then
    echo "ERROR: ITERATIONS must be a positive integer (got '$ITERATIONS')" >&2
    exit 2
fi

if ! perl -MTime::HiRes=time -e 'print time' >/dev/null 2>&1; then
    echo "ERROR: perl Time::HiRes is required for timing." >&2
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

tmp_root="$(mktemp -d "${TMPDIR:-/tmp}/sjulia-aot-cranelift-bench.XXXXXX")"
cleanup() {
    rm -rf "$tmp_root"
}
trap cleanup EXIT

now_seconds() {
    perl -MTime::HiRes=time -e 'printf "%.9f", time'
}

elapsed_seconds() {
    local start="$1"
    local end="$2"
    awk -v start="$start" -v end="$end" 'BEGIN { printf "%.6f", end - start }'
}

measure_command() {
    local stdout_path="$1"
    local stderr_path="$2"
    shift 2

    local start end
    start="$(now_seconds)"
    "$@" >"$stdout_path" 2>"$stderr_path"
    end="$(now_seconds)"
    elapsed_seconds "$start" "$end"
}

mean_seconds() {
    awk '
        { sum += $1; count += 1 }
        END {
            if (count == 0) {
                printf "n/a"
            } else {
                printf "%.6f", sum / count
            }
        }
    '
}

run_repeated() {
    local times_path="$1"
    local stdout_prefix="$2"
    local stderr_prefix="$3"
    shift 3

    : >"$times_path"
    for i in $(seq 1 "$ITERATIONS"); do
        measure_command "${stdout_prefix}.${i}" "${stderr_prefix}.${i}" "$@" >>"$times_path"
        printf '\n' >>"$times_path"
    done
    mean_seconds <"$times_path"
}

printf 'fixture\trust_check_mean_s\trust_emit_binary_s\trust_run_mean_s\trust_binary_bytes\tcranelift_check_mean_s\tcranelift_jit_run_mean_s\titerations\n'

for fixture in "$@"; do
    if [[ ! -f "$fixture" ]]; then
        echo "ERROR: fixture not found: $fixture" >&2
        exit 2
    fi

    fixture_dir="$tmp_root/$(basename "$fixture").d"
    mkdir -p "$fixture_dir"

    rust_check_mean="$(
        run_repeated \
            "$fixture_dir/rust-check.times" \
            "$fixture_dir/rust-check.out" \
            "$fixture_dir/rust-check.err" \
            "$JULIARS_BIN" "$fixture" --backend rust --check
    )"

    generated_rs="$fixture_dir/generated.rs"
    rust_bin="$fixture_dir/rust_backend_bin"
    rust_emit_binary_s="$(
        measure_command \
            "$fixture_dir/rust-emit.out" \
            "$fixture_dir/rust-emit.err" \
            "$JULIARS_BIN" "$fixture" -o "$generated_rs" --emit-binary "$rust_bin"
    )"

    rust_binary_bytes="$(wc -c <"$rust_bin" | tr -d '[:space:]')"

    rust_run_mean="$(
        run_repeated \
            "$fixture_dir/rust-run.times" \
            "$fixture_dir/rust-run.out" \
            "$fixture_dir/rust-run.err" \
            "$rust_bin"
    )"

    cranelift_check_mean="$(
        run_repeated \
            "$fixture_dir/cranelift-check.times" \
            "$fixture_dir/cranelift-check.out" \
            "$fixture_dir/cranelift-check.err" \
            "$JULIARS_BIN" "$fixture" --backend cranelift --check
    )"

    cranelift_jit_run_mean="$(
        run_repeated \
            "$fixture_dir/cranelift-jit-run.times" \
            "$fixture_dir/cranelift-jit-run.out" \
            "$fixture_dir/cranelift-jit-run.err" \
            "$JULIARS_BIN" "$fixture" --backend cranelift --jit-run
    )"

    printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
        "$fixture" \
        "$rust_check_mean" \
        "$rust_emit_binary_s" \
        "$rust_run_mean" \
        "$rust_binary_bytes" \
        "$cranelift_check_mean" \
        "$cranelift_jit_run_mean" \
        "$ITERATIONS"
done
