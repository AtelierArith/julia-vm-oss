#!/bin/bash
# Check for panic!() in VM execution paths
# This script helps prevent regressions where panic!() is used instead of proper
# error handling via raise() + VmError.
#
# Issue #1599: Prevention - Avoid panic!() in VM error paths
#
# Usage: ./scripts/check_panic_in_vm.sh
# Exit code: 0 if no issues found, 1 if panic! found in exec paths

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
VM_EXEC_DIR="$SCRIPT_DIR/../src/vm/exec"

echo "Checking for panic!() in VM execution paths..."

# Count panics in exec directory
PANIC_COUNT=$(grep -r "panic!" "$VM_EXEC_DIR" 2>/dev/null | grep -v "// allowed:" | wc -l | tr -d ' ')

if [ "$PANIC_COUNT" -gt 0 ]; then
    echo ""
    echo "WARNING: Found $PANIC_COUNT panic!() call(s) in VM exec paths:"
    echo ""
    grep -rn "panic!" "$VM_EXEC_DIR" 2>/dev/null | grep -v "// allowed:"
    echo ""
    echo "These should typically use self.raise(VmError::...) instead."
    echo "If the panic is intentional (truly impossible state), add '// allowed:' comment."
    echo ""
    exit 1
else
    echo "OK: No unallowed panic!() calls found in VM exec paths."
    exit 0
fi
