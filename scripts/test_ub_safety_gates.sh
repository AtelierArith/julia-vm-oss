#!/usr/bin/env bash
# Smoke tests for the UB-safety audit helpers (Issue #9004).
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
tmpdir="$(mktemp -d "${TMPDIR:-/tmp}/ub-safety-gates-test.XXXXXX")"
trap 'rm -rf "$tmpdir"' EXIT

mkdir -p "$tmpdir/repo/subset_julia_vm/src" "$tmpdir/repo/docs/vm"
cat > "$tmpdir/repo/subset_julia_vm/src/lib.rs" <<'RS'
pub fn unannotated(ptr: *const u8) -> u8 {
    unsafe { *ptr }
}

pub fn annotated(ptr: *const u8) -> u8 {
    // Safety: caller provides a valid byte pointer for this FFI probe (Issue #9004).
    unsafe { *ptr }
}
RS
printf 'fingerprint\tpath\tline\tkind\ttext\n' > "$tmpdir/repo/docs/vm/UNSAFE_INVENTORY_BASELINE.tsv"

if python3 "$ROOT_DIR/scripts/unsafe_inventory.py" \
    --root "$tmpdir/repo" \
    --out-dir "$tmpdir/out" \
    --baseline "$tmpdir/repo/docs/vm/UNSAFE_INVENTORY_BASELINE.tsv" \
    --check; then
  echo "ERROR: unsafe inventory accepted a new unannotated unsafe block" >&2
  exit 1
fi

python3 - "$tmpdir/out/unsafe_inventory.tsv" "$tmpdir/repo/docs/vm/UNSAFE_INVENTORY_BASELINE.tsv" <<'PY'
import csv
import sys

inventory, baseline = sys.argv[1:]
with open(inventory, newline="") as fh:
    rows = list(csv.DictReader(fh, delimiter="\t"))
unannotated = [row for row in rows if row["has_safety_issue_comment"] == "false"]
if len(unannotated) != 1:
    raise SystemExit(f"expected exactly one unannotated unsafe row, got {len(unannotated)}")
with open(baseline, "a", newline="") as fh:
    writer = csv.DictWriter(
        fh,
        fieldnames=["fingerprint", "path", "line", "kind", "text"],
        delimiter="\t",
        lineterminator="\n",
    )
    row = unannotated[0]
    writer.writerow({key: row[key] for key in ["fingerprint", "path", "line", "kind", "text"]})
PY

python3 "$ROOT_DIR/scripts/unsafe_inventory.py" \
  --root "$tmpdir/repo" \
  --out-dir "$tmpdir/out2" \
  --baseline "$tmpdir/repo/docs/vm/UNSAFE_INVENTORY_BASELINE.tsv" \
  --check

echo "OK: UB safety gate smoke tests passed"
