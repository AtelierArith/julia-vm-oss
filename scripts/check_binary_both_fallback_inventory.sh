#!/usr/bin/env bash
# Issue #4262 - keep CallDynamicBinaryBoth fallback ownership documented.

set -euo pipefail

source_file="subset_julia_vm/src/vm/exec/binary_both.rs"
doc_file="docs/vm/BINARY_DISPATCH.md"

tmpdir=$(mktemp -d "${TMPDIR:-/tmp}/binary-both-inventory.XXXXXX")
cleanup() {
    rm -rf "$tmpdir"
}
trap cleanup EXIT

code_tags="$tmpdir/code_tags"
doc_tags="$tmpdir/doc_tags"

rg -o 'BinaryBothFallback: [A-Za-z0-9_-]+' "$source_file" \
    | sed 's/^BinaryBothFallback: //' \
    | sort -u > "$code_tags"

rg -o 'BinaryBothFallback: [A-Za-z0-9_-]+' "$doc_file" \
    | sed 's/^BinaryBothFallback: //' \
    | sort -u > "$doc_tags"

errors=0

if [[ ! -s "$code_tags" ]]; then
    echo "ERROR: no BinaryBothFallback tags found in $source_file."
    errors=$((errors + 1))
fi

duplicate_doc="$tmpdir/duplicate_doc"
uniq -d "$doc_tags" > "$duplicate_doc"

if [[ -s "$duplicate_doc" ]]; then
    echo "ERROR: duplicate BinaryBothFallback tags in $doc_file:"
    cat "$duplicate_doc"
    errors=$((errors + 1))
fi

missing_doc="$tmpdir/missing_doc"
missing_code="$tmpdir/missing_code"
comm -23 "$code_tags" "$doc_tags" > "$missing_doc"
comm -13 "$code_tags" "$doc_tags" > "$missing_code"

if [[ -s "$missing_doc" ]]; then
    echo "ERROR: BinaryBothFallback tags in code but not documented in $doc_file:"
    cat "$missing_doc"
    errors=$((errors + 1))
fi

if [[ -s "$missing_code" ]]; then
    echo "ERROR: BinaryBothFallback tags documented but missing in $source_file:"
    cat "$missing_code"
    errors=$((errors + 1))
fi

if [[ "$errors" -gt 0 ]]; then
    echo ""
    echo "FAILED: CallDynamicBinaryBoth fallback inventory is out of sync (Issue #4262)."
    exit 1
fi

echo "OK: CallDynamicBinaryBoth fallback inventory is documented (Issue #4262)."
