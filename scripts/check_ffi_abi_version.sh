#!/usr/bin/env bash
# check_ffi_abi_version.sh — C ABI signature ratchet (Issue #9001).
#
# Guards against ABI-breaking changes to subset_vm.h that are not accompanied
# by a SUBSET_VM_ABI_VERSION bump.
#
# The script hashes all ABI-relevant declarations in the header (struct layouts,
# enum discriminants, function signatures) and compares the result against a
# stored baseline.  If the hash changes but SUBSET_VM_ABI_VERSION did not
# increase, CI fails.
#
# Normal check:
#   bash scripts/check_ffi_abi_version.sh
#
# Update the baseline after a deliberate ABI change + version bump:
#   bash scripts/check_ffi_abi_version.sh --update
#
# Ratchet design: the baseline file records both the hash and the ABI version
# that was current when the baseline was last updated.  Three failure modes:
#
#   1. Signature changed, version unchanged  →  FAIL (must bump version)
#   2. Signature changed, version bumped, baseline not updated  →  FAIL (run --update)
#   3. Signature unchanged  →  PASS
#
# Additionally, the script verifies that the Rust constant SUBSET_VM_C_ABI_VERSION
# in abi_version.rs matches the header macro SUBSET_VM_ABI_VERSION.

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
HEADER="$ROOT_DIR/subset_julia_vm_ffi/include/subset_vm.h"
ABI_RS="$ROOT_DIR/subset_julia_vm_ffi/src/abi_version.rs"
BASELINE="$ROOT_DIR/subset_julia_vm_ffi/abi_baseline"

UPDATE=0
if [[ "${1:-}" == "--update" ]]; then
    UPDATE=1
fi

# ---------------------------------------------------------------------------
# 1. Extract SUBSET_VM_ABI_VERSION from the header macro.
# ---------------------------------------------------------------------------
header_version="$(grep -E '^#define SUBSET_VM_ABI_VERSION ' "$HEADER" | sed 's/.*SUBSET_VM_ABI_VERSION[[:space:]]*//;s/u$//')"
if [[ -z "$header_version" ]]; then
    echo "ERROR: SUBSET_VM_ABI_VERSION macro not found in $HEADER" >&2
    exit 1
fi

# ---------------------------------------------------------------------------
# 2. Extract SUBSET_VM_C_ABI_VERSION from the Rust source and verify it matches.
# ---------------------------------------------------------------------------
rust_version="$(grep -E 'pub const SUBSET_VM_C_ABI_VERSION: u32 = ' "$ABI_RS" | sed 's/.*= //;s/;//')"
if [[ -z "$rust_version" ]]; then
    echo "ERROR: SUBSET_VM_C_ABI_VERSION constant not found in $ABI_RS" >&2
    exit 1
fi

if [[ "$header_version" != "$rust_version" ]]; then
    echo "ERROR: ABI version mismatch between header and Rust constant:" >&2
    echo "  subset_vm.h:      SUBSET_VM_ABI_VERSION = $header_version" >&2
    echo "  abi_version.rs:   SUBSET_VM_C_ABI_VERSION = $rust_version" >&2
    echo "" >&2
    echo "  Keep both values identical. See docs/vm/CHECKLISTS.md §\"ABI Change Checklist\"." >&2
    exit 1
fi

# ---------------------------------------------------------------------------
# 3. Hash ABI-relevant content: strip comments, then extract typedef struct,
#    typedef enum, and function declarations.
# ---------------------------------------------------------------------------
current_hash="$(python3 - "$HEADER" <<'PY'
import re
import sys
import hashlib

header = open(sys.argv[1]).read()

# Strip // line comments
header = re.sub(r'//[^\n]*', '', header)
# Strip /* ... */ block comments
header = re.sub(r'/\*.*?\*/', '', header, flags=re.DOTALL)
# Collapse whitespace for stable hashing
header = re.sub(r'\s+', ' ', header).strip()

# Extract ABI-relevant fragments only:
#   #define SUBSET_VM_ABI_VERSION ...
#   typedef struct { ... } Name;
#   typedef enum  { ... } Name;
#   function declarations (lines containing a type, identifier, and '(' without '{')
fragments = []

# Version macro
m = re.search(r'#define SUBSET_VM_ABI_VERSION\s+\S+', header)
if m:
    fragments.append(m.group(0))

# typedef struct / typedef enum blocks
for m in re.finditer(r'typedef\s+(?:struct|enum)\s*\{[^}]*\}\s*\w+\s*;', header):
    fragments.append(m.group(0))

# Function declarations: lines of the form "returntype name(params);"
# We look for tokens ending in ');\n' that are not inside struct/enum bodies.
# Work on the original (comment-stripped) source split by ';'.
for stmt in header.split(';'):
    stmt = stmt.strip()
    # Must contain '(' and ')' (looks like a function decl), and must not be
    # a typedef struct/enum (those are handled above) or a #define.
    if '(' in stmt and ')' in stmt and 'typedef' not in stmt and '#define' not in stmt:
        # Exclude OutputCallback typedef (function pointer type, not a decl)
        if 'OutputCallback' not in stmt or 'typedef' in stmt:
            # Simple heuristic: if it ends with ')' (after stripping) it is a decl
            candidate = stmt.rstrip()
            if candidate.endswith(')'):
                fragments.append(candidate + ';')

digest = hashlib.sha256('\n'.join(fragments).encode()).hexdigest()
print(digest)
PY
)"

if [[ -z "$current_hash" ]]; then
    echo "ERROR: failed to compute ABI signature hash from $HEADER" >&2
    exit 1
fi

# ---------------------------------------------------------------------------
# 4. Update mode: write the new baseline and exit.
# ---------------------------------------------------------------------------
if [[ "$UPDATE" -eq 1 ]]; then
    cat > "$BASELINE" <<BASELINE
# ABI version baseline for check_ffi_abi_version.sh (Issue #9001).
# Do NOT edit HASH or ABI_VERSION manually.
# Run: bash scripts/check_ffi_abi_version.sh --update   (after bumping SUBSET_VM_ABI_VERSION)
ABI_VERSION=$header_version
HASH=$current_hash
BASELINE
    echo "OK: abi_baseline updated (ABI_VERSION=$header_version, HASH=${current_hash:0:16}...)"
    exit 0
fi

# ---------------------------------------------------------------------------
# 5. Read the stored baseline.
# ---------------------------------------------------------------------------
if [[ ! -f "$BASELINE" ]]; then
    echo "ERROR: missing ABI baseline file: $BASELINE" >&2
    echo "  Run: bash scripts/check_ffi_abi_version.sh --update" >&2
    exit 1
fi

baseline_version="$(grep '^ABI_VERSION=' "$BASELINE" | cut -d= -f2)"
baseline_hash="$(grep '^HASH=' "$BASELINE" | cut -d= -f2)"

# ---------------------------------------------------------------------------
# 6. Compare.
# ---------------------------------------------------------------------------
if [[ "$current_hash" == "$baseline_hash" ]]; then
    echo "OK: C ABI signature unchanged (SUBSET_VM_ABI_VERSION=$header_version) (Issue #9001)"
    exit 0
fi

# Signature changed.
if [[ "$header_version" == "$baseline_version" ]]; then
    echo "ERROR: C ABI signature changed but SUBSET_VM_ABI_VERSION was not bumped." >&2
    echo "" >&2
    echo "  baseline HASH: $baseline_hash" >&2
    echo "  current  HASH: $current_hash" >&2
    echo "" >&2
    echo "  If you intentionally changed struct layouts, enum values, or function" >&2
    echo "  signatures, bump SUBSET_VM_ABI_VERSION in:" >&2
    echo "    subset_julia_vm_ffi/include/subset_vm.h" >&2
    echo "    subset_julia_vm_ffi/src/abi_version.rs" >&2
    echo "  then run: bash scripts/check_ffi_abi_version.sh --update" >&2
    echo "" >&2
    echo "  See docs/vm/CHECKLISTS.md §\"ABI Change Checklist\"." >&2
    exit 1
else
    echo "ERROR: C ABI signature changed and SUBSET_VM_ABI_VERSION was bumped to $header_version," >&2
    echo "  but the baseline file was not updated." >&2
    echo "" >&2
    echo "  Run: bash scripts/check_ffi_abi_version.sh --update" >&2
    exit 1
fi
