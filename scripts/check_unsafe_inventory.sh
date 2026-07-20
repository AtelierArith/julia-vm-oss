#!/usr/bin/env bash
# Gate new unannotated Rust unsafe sites (Issue #9004).
set -euo pipefail
cd "$(dirname "$0")/.."

out_dir="${1:-target/ub-safety}"
python3 scripts/unsafe_inventory.py \
  --out-dir "${out_dir}" \
  --baseline docs/vm/UNSAFE_INVENTORY_BASELINE.tsv \
  --check
