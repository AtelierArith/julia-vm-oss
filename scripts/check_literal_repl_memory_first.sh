#!/usr/bin/env bash
# Issue #3963 - Literal::Array* constant conversion and REPL persistence
# reconstruction must use Memory-first array materialization.

set -euo pipefail

errors=0

if rg -n 'ArrayValue::from_(f64|i64|bool)\(' subset_julia_vm/src/compile/expr/mod.rs subset_julia_vm/src/compile/utils.rs; then
    echo "ERROR: Literal::Array* conversion must use ArrayValue::memory_first_from_* helpers."
    errors=$((errors + 1))
fi

if rg -n 'ArrayData::(F64|I64|Bool)' subset_julia_vm/src/repl/converters.rs; then
    echo "ERROR: REPL array literal persistence must read logical elements, not raw ArrayData storage."
    errors=$((errors + 1))
fi

if ! rg -q 'get_linear' subset_julia_vm/src/repl/converters.rs; then
    echo "ERROR: REPL array literal persistence should read ArrayValue logical elements."
    errors=$((errors + 1))
fi

if [[ "$errors" -gt 0 ]]; then
    echo ""
    echo "FAILED: literal/REPL Memory-first audit failed (Issue #3963)."
    exit 1
fi

echo "OK: literal/REPL array reconstruction uses Memory-first/logical paths (Issue #3963)."
