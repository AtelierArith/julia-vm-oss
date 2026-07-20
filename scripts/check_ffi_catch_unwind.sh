#!/usr/bin/env bash
# Ensure every exported native C ABI function has an FFI panic boundary.
set -euo pipefail
cd "$(dirname "$0")/.."

out_dir="${1:-target/ffi-catch-unwind-audit}"
mkdir -p "${out_dir}"

summary="$(python3 scripts/panic_free_inventory.py --skip-clippy --out-dir "${out_dir}")"
printf '%s\n' "${summary}"

missing="$(printf '%s\n' "${summary}" | sed -nE 's/^ffi_missing_catch_unwind=([0-9]+)$/\1/p')"
if [[ -z "${missing}" ]]; then
  echo "ERROR: panic_free_inventory.py did not report ffi_missing_catch_unwind" >&2
  exit 2
fi
if [[ "${missing}" != "0" ]]; then
  echo "ERROR: ${missing} FFI exports lack catch_unwind; see ${out_dir}/ffi_catch_unwind.tsv" >&2
  exit 1
fi
