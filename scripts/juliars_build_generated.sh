#!/usr/bin/env bash
# Build a Rust source file emitted by `juliars` using a temporary Cargo project
# that links against the workspace `subset_julia_vm_runtime` crate.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

usage() {
  cat >&2 <<'EOF'
Usage:
  scripts/juliars_build_generated.sh <generated.rs> <output-binary>

Example:
  cargo run -p subset_julia_vm --bin juliars --features aot -- -e '1 + 2' -o /tmp/program.rs
  scripts/juliars_build_generated.sh /tmp/program.rs /tmp/program
EOF
}

if [[ "$#" -ne 2 ]]; then
  usage
  exit 2
fi

generated_rs="$1"
output_bin="$2"

if [[ ! -f "$generated_rs" ]]; then
  echo "error: generated Rust source not found: $generated_rs" >&2
  exit 3
fi

tmp_dir="$(mktemp -d "${TMPDIR:-/tmp}/sjulia-aot-link.XXXXXX")"
cleanup() {
  rm -rf "$tmp_dir"
}
trap cleanup EXIT

mkdir -p "$tmp_dir/src"
cp "$generated_rs" "$tmp_dir/src/main.rs"

cat > "$tmp_dir/Cargo.toml" <<EOF
[package]
name = "sjulia_aot_generated"
version = "0.1.0"
edition = "2021"

[dependencies]
subset_julia_vm_runtime = { path = "$ROOT/subset_julia_vm_runtime" }
EOF

CARGO_TARGET_DIR="$tmp_dir/target" \
  timeout 1800 cargo build --release --manifest-path "$tmp_dir/Cargo.toml"

mkdir -p "$(dirname "$output_bin")"
cp "$tmp_dir/target/release/sjulia_aot_generated" "$output_bin"
echo "Built: $output_bin"
