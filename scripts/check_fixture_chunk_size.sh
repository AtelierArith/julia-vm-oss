#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BUILD_RS="$ROOT_DIR/subset_julia_vm/build.rs"
FIXTURE_DIR="$ROOT_DIR/subset_julia_vm/tests/fixtures"
MIN_BATCH_SIZE=32

batch_size="$(sed -n 's/.*const FIXTURE_BATCH_SIZE: usize = \([0-9][0-9]*\);.*/\1/p' "$BUILD_RS")"
if [[ -z "$batch_size" ]]; then
  echo "ERROR: could not find FIXTURE_BATCH_SIZE in $BUILD_RS" >&2
  exit 1
fi

if [[ "$batch_size" -lt "$MIN_BATCH_SIZE" ]]; then
  echo "ERROR: FIXTURE_BATCH_SIZE is $batch_size; expected at least $MIN_BATCH_SIZE for Issue #3972" >&2
  exit 1
fi

total_tests=0
total_chunks=0
categories=0

for manifest in "$FIXTURE_DIR"/*/manifest.toml; do
  if [[ ! -f "$manifest" ]]; then
    continue
  fi

  count="$(grep -c '^\[\[tests\]\]' "$manifest" || true)"
  if [[ "$count" -eq 0 ]]; then
    continue
  fi

  chunks=$(((count + batch_size - 1) / batch_size))
  total_tests=$((total_tests + count))
  total_chunks=$((total_chunks + chunks))
  categories=$((categories + 1))
done

if [[ "$total_tests" -eq 0 ]]; then
  echo "ERROR: no fixture manifest entries found under $FIXTURE_DIR" >&2
  exit 1
fi

echo "OK: FIXTURE_BATCH_SIZE=$batch_size; $total_tests fixture manifest entries generate $total_chunks chunk tests across $categories categories (Issue #3972)"
