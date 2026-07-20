#!/usr/bin/env bash
# Issue #4052 - keep retained native collect fallback boundaries documented.

set -euo pipefail

source_files=(
    "subset_julia_vm_compile/src/compile/expr/builtin.rs"
    "subset_julia_vm_compile/src/compile/expr/call/mod.rs"
    "subset_julia_vm_compile/src/compile/expr/call/handlers/arrays.rs"
    "subset_julia_vm_vm/src/vm/builtins_exec.rs"
    "subset_julia_vm_vm/src/vm/exec/call_dynamic.rs"
    "subset_julia_vm_vm/src/vm/type_ops/iteration.rs"
)
doc_file="docs/vm/COLLECTIONS.md"

tmpdir=$(mktemp -d "${TMPDIR:-/tmp}/collect-fallback-inventory.XXXXXX")
cleanup() {
    rm -rf "$tmpdir"
}
trap cleanup EXIT

code_tags="$tmpdir/code_tags"
doc_tags="$tmpdir/doc_tags"

rg -o 'CollectFallback: [A-Za-z0-9_-]+' "${source_files[@]}" \
    | sed 's/^.*CollectFallback: //' \
    | sort > "$code_tags"

rg -o 'CollectFallback: [A-Za-z0-9_-]+' "$doc_file" \
    | sed 's/^CollectFallback: //' \
    | sort > "$doc_tags"

errors=0

if [[ ! -s "$code_tags" ]]; then
    echo "ERROR: no CollectFallback tags found in collect fallback source files."
    errors=$((errors + 1))
fi

duplicate_code="$tmpdir/duplicate_code"
duplicate_doc="$tmpdir/duplicate_doc"
uniq -d "$code_tags" > "$duplicate_code"
uniq -d "$doc_tags" > "$duplicate_doc"

if [[ -s "$duplicate_code" ]]; then
    echo "ERROR: duplicate CollectFallback tags in source files:"
    cat "$duplicate_code"
    errors=$((errors + 1))
fi

if [[ -s "$duplicate_doc" ]]; then
    echo "ERROR: duplicate CollectFallback tags in $doc_file:"
    cat "$duplicate_doc"
    errors=$((errors + 1))
fi

missing_doc="$tmpdir/missing_doc"
missing_code="$tmpdir/missing_code"
comm -23 "$code_tags" "$doc_tags" > "$missing_doc"
comm -13 "$code_tags" "$doc_tags" > "$missing_code"

if [[ -s "$missing_doc" ]]; then
    echo "ERROR: CollectFallback tags in code but not documented in $doc_file:"
    cat "$missing_doc"
    errors=$((errors + 1))
fi

if [[ -s "$missing_code" ]]; then
    echo "ERROR: CollectFallback tags documented but missing in source files:"
    cat "$missing_code"
    errors=$((errors + 1))
fi

if [[ "$errors" -gt 0 ]]; then
    echo ""
    echo "FAILED: collect fallback inventory is out of sync (Issue #4052)."
    exit 1
fi

echo "OK: collect fallback inventory is documented (Issue #4052)."
