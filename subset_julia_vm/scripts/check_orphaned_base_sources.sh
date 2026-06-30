#!/bin/bash
# Check for orphaned .jl files in src/julia/base/
#
# This script ensures all Julia source files in the base directory are either:
# 1. Loaded via include_str! in mod.rs, OR
# 2. Explicitly excluded in the EXCLUDED_FILES list
#
# Usage: ./check_orphaned_base_sources.sh
# Exit code: 0 if all files accounted for, 1 if orphaned files found

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BASE_DIR="$SCRIPT_DIR/../src/julia/base"
MOD_RS="$BASE_DIR/mod.rs"

# Files that are intentionally NOT loaded into BASE_SOURCES
# Add files here with a comment explaining why they're excluded
# Format: one file per line
EXCLUDED_FILES="exports.jl"
# exports.jl contains export declarations (metadata), not executable code.
# The VM handles exports differently than Julia's module system.

# Helper function to check if a file is excluded
is_excluded() {
    local file="$1"
    echo "$EXCLUDED_FILES" | grep -q "^${file}$"
}

# Count excluded files
count_excluded() {
    echo "$EXCLUDED_FILES" | grep -c . || echo "0"
}

# Find all .jl files in base directory
ALL_JL_FILES=$(find "$BASE_DIR" -name "*.jl" | sed "s|$BASE_DIR/||" | sort)

# Extract files loaded in mod.rs via include_str!
LOADED_FILES=$(grep -o 'include_str!("[^"]*\.jl")' "$MOD_RS" | sed 's/include_str!("//g; s/")//' | sort)

# Check for orphaned files
ORPHANED=""
for file in $ALL_JL_FILES; do
    # Skip if file is loaded
    if echo "$LOADED_FILES" | grep -q "^${file}$"; then
        continue
    fi

    # Skip if file is explicitly excluded
    if is_excluded "$file"; then
        continue
    fi

    if [ -z "$ORPHANED" ]; then
        ORPHANED="$file"
    else
        ORPHANED="$ORPHANED
$file"
    fi
done

if [ -n "$ORPHANED" ]; then
    echo "ERROR: Found orphaned Julia source files in src/julia/base/"
    echo ""
    echo "The following files are not loaded in mod.rs and not explicitly excluded:"
    echo "$ORPHANED" | while read -r file; do
        echo "  - $file"
    done
    echo ""
    echo "To fix this, either:"
    echo "  1. Add the file to BASE_SOURCES in mod.rs (include_str! + get_base())"
    echo "  2. Add the file to EXCLUDED_FILES in this script with a justification"
    echo ""
    echo "See Issue #1765 and #1770 for context on why this check exists."
    exit 1
fi

LOADED_COUNT=$(echo "$LOADED_FILES" | wc -l | tr -d ' ')
EXCLUDED_COUNT=$(count_excluded)

echo "OK: All Julia source files in src/julia/base/ are accounted for."
echo "  - Loaded: $LOADED_COUNT files"
echo "  - Excluded: $EXCLUDED_COUNT files"
exit 0
