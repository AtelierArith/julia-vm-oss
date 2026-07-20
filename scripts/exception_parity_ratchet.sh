#!/usr/bin/env bash
# Runtime differential lane for Issue #10813 / Phase 3 #11148.
# Builds the REPL binary, probes the same constructs under Julia and sjulia,
# then rejects new divergences and stale issue-linked allowlist rows.

set -euo pipefail

cd "$(dirname "$0")/.."

SJULIA_BIN="${SJULIA_EXCEPTION_PARITY_BIN:-target/release/sjulia}"
JULIA_CMD="${SJULIA_UPSTREAM_JULIA:-julia}"
ALLOWLIST="${SJULIA_EXCEPTION_PARITY_ALLOWLIST:-docs/vm/EXCEPTION_PARITY_ALLOWLIST.tsv}"
OUT=""
BUILD=1

while [ "$#" -gt 0 ]; do
  case "$1" in
    --sjulia) SJULIA_BIN="$2"; BUILD=0; shift 2 ;;
    --julia) JULIA_CMD="$2"; shift 2 ;;
    --allowlist) ALLOWLIST="$2"; shift 2 ;;
    --out) OUT="$2"; shift 2 ;;
    --no-build) BUILD=0; shift ;;
    *) echo "FAIL: unknown argument: $1" >&2; exit 2 ;;
  esac
done

if [ "$BUILD" -eq 1 ]; then
  cargo build --locked --release -p subset_julia_vm --bin sjulia --features repl
fi
if [ ! -x "$SJULIA_BIN" ]; then
  echo "FAIL: sjulia binary is not executable: $SJULIA_BIN" >&2
  exit 1
fi

report="$(mktemp)"
trap 'rm -f "$report"' EXIT
python3 scripts/exception_parity_probe.py \
  --sjulia "$SJULIA_BIN" --julia "$JULIA_CMD" --out "$report"
checker_args=(--report "$report" --allowlist "$ALLOWLIST")
if [ -z "$OUT" ]; then
  # A normal gate pins the exact corpus membership. `--out` is the explicit
  # maintainer path for intentionally refreshing that committed membership.
  checker_args+=(--case-baseline docs/vm/EXCEPTION_PARITY_PROBE.tsv)
fi
python3 scripts/exception_parity_ratchet.py "${checker_args[@]}"

if [ -n "$OUT" ]; then
  cp "$report" "$OUT"
  echo "updated exception-parity report: $OUT"
fi
