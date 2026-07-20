#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

JULIA_BIN="${JULIA_BIN:-julia}"

python3 "$ROOT/scripts/repl_session_julia_oracle.py" --julia "$JULIA_BIN" --check "$@"
timeout 1800 cargo nextest run --release -p subset_julia_vm --features repl --test repl_session_fixture_tests
