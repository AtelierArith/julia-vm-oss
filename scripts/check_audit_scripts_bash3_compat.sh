#!/usr/bin/env bash
# Reject bash 4+ only constructs in scripts/check_*.sh so that audit scripts
# stay runnable on macOS's default /bin/bash 3.2.57.
#
# Background: every developer Issue with the workaround / Debug-derive audits
# was the same shape — `mapfile` is a bash 4 builtin; macOS ships bash 3.2;
# CI runs Linux bash 4 so the breakage was invisible at PR time. See:
#   - Issue #3759 (check_workarounds_sync.sh, fixed by PR #3769)
#   - Issue #3766 (check_missing_debug.sh, fixed by PR #3773)
#   - Issue #3771 (this audit)
#
# Forbidden constructs:
#   - `mapfile` / `readarray` builtins (bash 4)
#
# When this audit fires, replace the bash-4 builtin with a portable loop:
#
#   arr=()
#   while IFS= read -r line; do arr+=("$line"); done < <(producer)
#
# and guard subsequent `"${arr[@]}"` splats with `"${arr[@]+"${arr[@]}"}"`
# so `set -u` does not trip on an empty array under bash 3.2.

set -euo pipefail

ERRORS=0

# Self-skip: the help text of this very script names the forbidden patterns,
# which would otherwise self-match. Compare by basename so the audit still
# detects violations when invoked from a sibling working directory.
SELF="$(basename "$0")"

for f in scripts/check_*.sh; do
    [[ -f "$f" ]] || continue
    if [[ "$(basename "$f")" == "$SELF" ]]; then
        continue
    fi

    # Match `mapfile` or `readarray` as a word — most often as the leading
    # command on a line, but also catches uses chained after `;`, `&&`, `|`.
    # Strip pure-comment lines first (replacing them with empty content while
    # preserving line numbering via `awk`) so explanation comments naming the
    # forbidden builtin do not self-trigger.
    matches=$(awk '/^[[:space:]]*#/ { print ""; next } { print }' "$f" \
        | grep -nE '\b(mapfile|readarray)\b' || true)
    if [[ -n "$matches" ]]; then
        echo "ERROR: $f uses a bash 4+ builtin (mapfile/readarray):"
        printf '%s\n' "$matches" | sed 's/^/    /'
        ERRORS=$((ERRORS + 1))
    fi
done

if [[ "$ERRORS" -gt 0 ]]; then
    echo ""
    echo "FAILED: $ERRORS audit script(s) contain bash 4+ constructs (Issue #3771)."
    echo "macOS ships /bin/bash 3.2.57; the audit workflow in CLAUDE.md prescribes"
    echo "running these scripts locally, so they must work on stock macOS."
    echo ""
    echo "Replace mapfile/readarray with the portable form:"
    echo ""
    echo "  arr=()"
    echo "  while IFS= read -r line; do arr+=(\"\$line\"); done < <(producer)"
    echo ""
    echo "and guard later \"\${arr[@]}\" splats with \"\${arr[@]+\"\${arr[@]}\"}\"."
    exit 1
fi

echo "OK: no bash 4+ constructs in scripts/check_*.sh (Issue #3771)."
