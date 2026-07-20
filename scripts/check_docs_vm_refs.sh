#!/usr/bin/env bash
# check_docs_vm_refs.sh
#
# Verify that every docs/vm/*.md file referenced in CLAUDE.md actually exists.
#
# CLAUDE.md Code Audits section says "See FOO.md" for various docs/vm/ files.
# If a file is renamed or deleted without updating CLAUDE.md, the reference
# becomes dangling. This script detects that (Issue #3118).
#
# Also verifies the Key References table at the bottom of CLAUDE.md.
#
# Usage: run from the repository root
#   bash scripts/check_docs_vm_refs.sh
#
# Exit code: 0 = all references valid, 1 = dangling references found

set -euo pipefail

DOCS_VM="docs/vm"
CLAUDE="CLAUDE.md"

if [[ ! -f "$CLAUDE" ]]; then
    echo "ERROR: $CLAUDE not found. Run this script from the repository root."
    exit 1
fi

if [[ ! -d "$DOCS_VM" ]]; then
    echo "ERROR: $DOCS_VM directory not found. Run this script from the repository root."
    exit 1
fi

# Extract all ALLCAPS*.md references from CLAUDE.md.
# Matches bare docs/vm-style names like STATUS.md, BUILTIN_OWNERSHIP.md,
# TYPE_SYSTEM.md, and path-qualified references like docs/vm/STATUS.md.
# Path-qualified references outside docs/vm (for example
# .agents/skills/<name>/SKILL.md) are intentionally ignored.
#
# After extraction, we filter out placeholder-style names (FOO.md, BAR.md, etc.)
# that appear in documentation examples but are not meant to be real file references.
# A name is considered a placeholder if it does NOT exist in docs/vm/ AND it is a
# common documentation placeholder word. This avoids both false positives from
# example text and the need to hardcode individual exclusions.
refs=$(
    grep -oE '([.A-Za-z0-9_<>{}-]+/)+[A-Z][A-Z0-9_]+\.md|[A-Z][A-Z0-9_]+\.md' "$CLAUDE" \
        | while IFS= read -r ref; do
            if [[ "$ref" == */* ]]; then
                [[ "$ref" == "$DOCS_VM/"* ]] || continue
                ref="${ref#"$DOCS_VM/"}"
            fi
            # bash 3.2 mis-parses a `case`/`;;` nested inside this `$(...)`
            # command substitution ("syntax error near unexpected token `;;'"),
            # so use an `if` guard instead (Issue #9461).
            if [[ "$ref" == CLAUDE.md || "$ref" == AGENTS.md \
                || "$ref" == GEMINI.md || "$ref" == SKILL.md \
                || "$ref" == REPOSITORY_RULES.md || "$ref" == MEMORY.md ]]; then
                continue
            fi
            printf '%s\n' "$ref"
        done \
        | sort -u
)

# Common placeholder names used in documentation examples.
# These are only excluded when they do NOT correspond to an actual file in docs/vm/.
PLACEHOLDERS="FOO BAR BAZ QUX QUUX EXAMPLE SAMPLE TEST PLACEHOLDER DUMMY MYFILE TEMPLATE"

is_placeholder() {
    local basename="${1%.md}"
    for p in $PLACEHOLDERS; do
        [[ "$basename" == "$p" ]] && return 0
    done
    return 1
}

missing=()
for ref in $refs; do
    if [[ ! -f "$DOCS_VM/$ref" ]]; then
        # Skip known documentation placeholder names
        if is_placeholder "$ref"; then
            continue
        fi
        missing+=("$ref")
    fi
done

if [[ ${#missing[@]} -gt 0 ]]; then
    echo "ERROR: the following files are referenced in $CLAUDE but not found in $DOCS_VM/:"
    for m in "${missing[@]}"; do
        echo "  $DOCS_VM/$m  (missing)"
        # Show where in CLAUDE.md it's referenced
        grep -n "$m" "$CLAUDE" | sed 's/^/    CLAUDE.md:/'
    done
    echo ""
    echo "Fix: create the missing file or update the reference in $CLAUDE."
    exit 1
fi

echo "OK: all docs/vm/ references in $CLAUDE are valid (Issue #3118)."
