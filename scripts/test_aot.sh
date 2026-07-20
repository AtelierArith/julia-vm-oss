#!/usr/bin/env bash
# Run the AoT-gated test suite, crate clippy, and generated-Rust clippy with
# `--features aot` enabled.
#
# Why this exists (Issue #6679):
#   The `aot` module is `#[cfg(feature = "aot")]`-gated. The normal local gate
#   `cargo nextest run --release` uses the default (empty) feature set, so AoT
#   code is never built or run there — AoT codegen regressions (e.g. the dead
#   `-> Value` Base helpers in #6629, or the clippy reds + e2e fails in #5658)
#   slip past it. This repo has no PR CI, so the local full suite is the only
#   gate. Run this script whenever a change touches the AoT pipeline (and
#   periodically) so AoT regressions are caught before they reach `main`.
#
# Issue #10815: the acceptance-kernel-only test_aot.sh + `aot_e2e_tests` gate
# proved structurally blind to a cluster of VM/AoT semantic-drift bugs (5 found
# in one week: #10796/#10731/#10663/#10537/#10523, plus #11180/#11181/#11182
# found while widening this gate) because nothing ran the differential
# `vm_aot` lane of `scripts/metamorphic_equivalence.sh` as part of the
# mandatory AoT gate — it was opt-in via `premerge_gate.sh --metamorphic`
# (auto-selected for `subset_julia_vm/src/*` changes at LEAD certification
# time only). Steps [4/8]-[6/8] below make the widened `vm_aot` corpus and its
# negative self-test part of the same gate every AoT-touching change already
# runs locally, closing the "audit exists but nothing runs it" gap (the same
# failure class as #10870/#10912). `scripts/check_test_aot_vm_aot_lane.sh`
# (registered in `scripts/source_only_audits.tsv`) pins that these steps
# cannot be silently removed from this script and that the corpus cannot
# silently shrink back toward the 3 acceptance kernels.
#
# Usage:
#   scripts/test_aot.sh                    # full AoT suite + numeric matrix + vm_aot lane + clippy
#   scripts/test_aot.sh --no-clippy        # skip the clippy pass
#   scripts/test_aot.sh --no-metamorphic   # skip the vm_aot differential lane + its selftest
#   scripts/test_aot.sh aot_e2e_tests      # forward a nextest filter
#
# Notes:
#   * nextest filters match on "binary test" (space-separated), NOT
#     "binary::test". Pass a bare test-function name; an `aot_e2e_tests::...`
#     form matches 0 tests.
#   * All non-flag arguments are forwarded verbatim to `cargo nextest run`.
#   * `SJULIA_BIN` / `JULIARS_BIN` default to the release binaries under
#     `CARGO_TARGET_DIR`; explicit binary overrides take precedence (Issue #11598).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "$ROOT/scripts/cargo_target_dir.sh"
cargo_target_dir="$(resolve_cargo_target_dir "$ROOT")"
export CARGO_TARGET_DIR="$cargo_target_dir"
SJULIA_BIN="${SJULIA_BIN:-$cargo_target_dir/release/sjulia}"
JULIARS_BIN="${JULIARS_BIN:-$cargo_target_dir/release/juliars}"
export SJULIA_BIN JULIARS_BIN

RUN_CLIPPY=1
RUN_METAMORPHIC=1
NEXTEST_ARGS=()
for arg in "$@"; do
  case "$arg" in
    --no-clippy) RUN_CLIPPY=0 ;;
    --no-metamorphic) RUN_METAMORPHIC=0 ;;
    *) NEXTEST_ARGS+=("$arg") ;;
  esac
done

echo "== [1/8] cargo nextest run --locked --release -p subset_julia_vm --features aot =="
# ${arr[@]+...} guards the empty-array expansion under `set -u` on bash 3.2 (macOS).
timeout 1800 cargo nextest run --locked --release -p subset_julia_vm --features aot \
  --no-fail-fast ${NEXTEST_ARGS[@]+"${NEXTEST_ARGS[@]}"}

echo "== [2/8] cargo build --locked --release -p subset_julia_vm --features aot --bin juliars =="
timeout 1800 cargo build --locked --release -p subset_julia_vm --features aot --bin juliars

echo "== [3/8] AoT reduced numeric matrix =="
timeout 1800 bash "$ROOT/scripts/aot_numeric_matrix_reduced.sh"

if [[ "$RUN_METAMORPHIC" -eq 1 ]]; then
  echo "== [4/8] cargo build --locked --release -p subset_julia_vm --bin sjulia --features repl (VM side of the vm_aot lane) =="
  timeout 1800 cargo build --locked --release -p subset_julia_vm --bin sjulia --features repl

  echo "== [5/8] vm_aot differential equivalence lane (Issue #10815) =="
  timeout 1800 bash "$ROOT/scripts/metamorphic_equivalence.sh" --lane vm_aot

  echo "== [6/8] vm_aot lane negative self-test (Issue #10815) =="
  timeout 600 bash "$ROOT/scripts/metamorphic_equivalence.sh" --selftest
else
  echo "== [4/8] sjulia build skipped (--no-metamorphic) =="
  echo "== [5/8] vm_aot differential lane skipped (--no-metamorphic) =="
  echo "== [6/8] vm_aot lane negative self-test skipped (--no-metamorphic) =="
fi

if [[ "$RUN_CLIPPY" -eq 1 ]]; then
  echo "== [7/8] registered AoT Clippy lane =="
  timeout 1800 bash "$ROOT/scripts/run_clippy_lanes.sh" aot

  echo "== [8/8] generated Rust cargo clippy =="
  tmp_dir="$(mktemp -d "${TMPDIR:-/tmp}/sjulia-aot-clippy.XXXXXX")"
  trap 'rm -rf "$tmp_dir"' EXIT
  "$JULIARS_BIN" \
    -e $'a = 3.0\nb = 2\nprintln(a + b)\n' \
    -o "$tmp_dir/generated.rs"
  mkdir -p "$tmp_dir/src"
  mv "$tmp_dir/generated.rs" "$tmp_dir/src/main.rs"
  cat > "$tmp_dir/Cargo.toml" <<EOF
[package]
name = "sjulia_aot_generated_clippy_smoke"
version = "0.1.0"
edition = "2021"

[dependencies]
subset_julia_vm_runtime = { path = "$ROOT/subset_julia_vm_runtime" }
EOF
  timeout 1800 cargo clippy --manifest-path "$tmp_dir/Cargo.toml" -- -D warnings
else
  echo "== [7/8] clippy skipped (--no-clippy) =="
  echo "== [8/8] generated Rust clippy skipped (--no-clippy) =="
fi

echo "== AoT gate: OK =="
