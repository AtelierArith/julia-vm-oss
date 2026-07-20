#!/usr/bin/env bash
# Run the practical miri VM smoke gate (Issue #9004).
set -euo pipefail
cd "$(dirname "$0")/.."

skip_if_unavailable=0
if [[ "${1:-}" == "--skip-if-unavailable" ]]; then
  skip_if_unavailable=1
fi

toolchain="${MIRI_TOOLCHAIN:-nightly}"

if ! cargo +"${toolchain}" miri --version >/dev/null 2>&1; then
  if [[ "${skip_if_unavailable}" -eq 1 ]]; then
    echo "SKIP: cargo +${toolchain} miri is unavailable"
    exit 0
  fi
  echo "ERROR: cargo +${toolchain} miri is unavailable." >&2
  echo "Install with: rustup toolchain install ${toolchain} --component miri rust-src" >&2
  exit 2
fi

export MIRIFLAGS="${MIRIFLAGS:--Zmiri-disable-isolation}"

cargo +"${toolchain}" miri test \
  -p subset_julia_vm \
  --test miri_smoke_tests
