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
# Usage:
#   scripts/test_aot.sh                    # full AoT suite + clippy
#   scripts/test_aot.sh --no-clippy        # skip the clippy pass
#   scripts/test_aot.sh aot_e2e_tests      # forward a nextest filter
#
# Notes:
#   * nextest filters match on "binary test" (space-separated), NOT
#     "binary::test". Pass a bare test-function name; an `aot_e2e_tests::...`
#     form matches 0 tests.
#   * All non-flag arguments are forwarded verbatim to `cargo nextest run`.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target}"

RUN_CLIPPY=1
NEXTEST_ARGS=()
for arg in "$@"; do
  case "$arg" in
    --no-clippy) RUN_CLIPPY=0 ;;
    *) NEXTEST_ARGS+=("$arg") ;;
  esac
done

echo "== [1/3] cargo nextest run --release -p subset_julia_vm --features aot =="
# ${arr[@]+...} guards the empty-array expansion under `set -u` on bash 3.2 (macOS).
timeout 1800 cargo nextest run --release -p subset_julia_vm --features aot \
  --no-fail-fast ${NEXTEST_ARGS[@]+"${NEXTEST_ARGS[@]}"}

if [[ "$RUN_CLIPPY" -eq 1 ]]; then
  echo "== [2/3] cargo clippy -p subset_julia_vm --features aot --all-targets =="
  timeout 1800 cargo clippy -p subset_julia_vm --features aot --all-targets -- -D warnings

  echo "== [3/3] generated Rust cargo clippy =="
  timeout 1800 cargo build --release -p subset_julia_vm --features aot --bin juliars
  tmp_dir="$(mktemp -d "${TMPDIR:-/tmp}/sjulia-aot-clippy.XXXXXX")"
  trap 'rm -rf "$tmp_dir"' EXIT
  "$ROOT/target/release/juliars" \
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
  echo "== [2/3] clippy skipped (--no-clippy) =="
  echo "== [3/3] generated Rust clippy skipped (--no-clippy) =="
fi

echo "== AoT gate: OK =="
