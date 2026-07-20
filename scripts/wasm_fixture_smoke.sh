#!/usr/bin/env bash
# Run a small fixture subset through subset_julia_vm_web under Node.
# Issue #8710, parent #8688.

set -euo pipefail
cd "$(dirname "$0")/.."

FIXTURE_TSV="${FIXTURE_TSV:-docs/vm/WASM_FIXTURE_SMOKE.tsv}"
ALLOWLIST_TSV="${ALLOWLIST_TSV:-docs/vm/WASM_FIXTURE_SMOKE_ALLOWLIST.tsv}"
FIXTURES_ROOT="${FIXTURES_ROOT:-subset_julia_vm/tests/fixtures}"
OUT_DIR="${OUT_DIR:-target/wasm-fixture-smoke}"
PKG_DIR="${PKG_DIR:-$OUT_DIR/pkg}"

usage() {
  cat <<'USAGE'
Usage:
  scripts/wasm_fixture_smoke.sh [--list] [--skip-build]

Environment:
  FIXTURE_TSV    fixture list TSV (default: docs/vm/WASM_FIXTURE_SMOKE.tsv)
  ALLOWLIST_TSV  known failure TSV (default: docs/vm/WASM_FIXTURE_SMOKE_ALLOWLIST.tsv)
  FIXTURES_ROOT  fixture root (default: subset_julia_vm/tests/fixtures)
  OUT_DIR        output directory (default: target/wasm-fixture-smoke)
  PKG_DIR        wasm-pack package directory (default: $OUT_DIR/pkg)
USAGE
}

skip_build=0
list_only=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --list)
      list_only=1
      shift
      ;;
    --skip-build)
      skip_build=1
      shift
      ;;
    --help|-h)
      usage
      exit 0
      ;;
    *)
      echo "ERROR: unknown argument: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

if [[ ! -f "$FIXTURE_TSV" ]]; then
  echo "ERROR: fixture TSV not found: $FIXTURE_TSV" >&2
  exit 1
fi

if [[ "$list_only" -eq 1 ]]; then
  awk -F '\t' 'NR > 1 && NF >= 3 && $1 !~ /^#/ { print $1 "\t" $2 "\t" $3 }' "$FIXTURE_TSV"
  exit 0
fi

if [[ ! -f "$ALLOWLIST_TSV" ]]; then
  echo "ERROR: allowlist TSV not found: $ALLOWLIST_TSV" >&2
  exit 1
fi

mkdir -p "$OUT_DIR"

if [[ "$skip_build" -eq 0 ]]; then
  wasm-pack build --target nodejs --profile web-release --out-dir "../$PKG_DIR" subset_julia_vm_web
fi

set +e
node scripts/wasm_fixture_runner.mjs "$PKG_DIR" "$FIXTURE_TSV" "$FIXTURES_ROOT" "$ALLOWLIST_TSV" \
  | tee "$OUT_DIR/results.tsv"
runner_status="${PIPESTATUS[0]}"
set -e

awk -F '\t' '$3 != "ok" { print }' "$OUT_DIR/results.tsv" > "$OUT_DIR/diff.tsv"
exit "$runner_status"
