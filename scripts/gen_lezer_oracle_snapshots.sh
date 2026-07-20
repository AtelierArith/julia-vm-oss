#!/usr/bin/env bash
# gen_lezer_oracle_snapshots.sh — regenerate the Canonical CST oracle
# snapshots in subset_julia_vm_parser_common/tests/oracle_snapshots/ from the
# lezer-julia reference grammar (Issue #11049, Phase 0).
#
# Requires Node.js and a built extern/lezer-julia:
#   bash scripts/populate_extern.sh lezer-julia
#   (cd extern/lezer-julia && npm install)   # npm install also builds dist/
#
# The generated snapshots are COMMITTED so the Rust differential tests run
# without Node.js. Rerun this script only when extern/lezer-julia is updated
# (then update extern/MANIFEST.tsv in the same PR).

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

if ! command -v node >/dev/null 2>&1; then
  echo "ERROR: node not found; the oracle generator requires Node.js" >&2
  exit 2
fi

if [[ ! -f "$REPO_ROOT/extern/lezer-julia/dist/index.js" ]]; then
  echo "ERROR: extern/lezer-julia is not built. Run:" >&2
  echo "  bash scripts/populate_extern.sh lezer-julia" >&2
  echo "  (cd extern/lezer-julia && npm install)" >&2
  exit 2
fi

node "$REPO_ROOT/tools/lezer-oracle-snapshots.mjs"
