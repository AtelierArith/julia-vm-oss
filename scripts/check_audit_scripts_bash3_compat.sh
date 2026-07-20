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
# Forbidden constructs (Issue #3771, extended in Issue #9461):
#   - `mapfile` / `readarray` builtins (bash 4)
#   - `declare -A` / `local -A` / `typeset -A` associative arrays (bash 4)
#   - `declare -g` global scope flag (bash 4)
#   - any script that fails `bash -n` under the invoking shell (a bash-3.2 parse
#     error, e.g. double quotes nested inside a double-quoted `$(...)` — the
#     shape that silently broke check_instr_wire_ids.sh on macOS, Issue #9461)
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

    # Match the bash 4+ constructs as words — most often the leading command on
    # a line, but also catches uses chained after `;`, `&&`, `|`. Strip
    # pure-comment lines first (replacing them with empty content while
    # preserving line numbering via `awk`) so explanation comments naming a
    # forbidden construct do not self-trigger.
    #   - mapfile / readarray                 : bash 4 builtins
    #   - declare/local/typeset with -A       : associative arrays (bash 4)
    #   - declare -g                          : global scope flag (bash 4)
    stripped=$(awk '/^[[:space:]]*#/ { print ""; next } { print }' "$f")
    matches=$(printf '%s\n' "$stripped" | grep -nE \
        '\b(mapfile|readarray)\b|\b(declare|local|typeset)[[:space:]]+(-[A-Za-z]*[Ag][A-Za-z]*)' \
        || true)
    if [[ -n "$matches" ]]; then
        echo "ERROR: $f uses a bash 4+ construct (mapfile/readarray/declare -A/-g):"
        printf '%s\n' "$matches" | sed 's/^/    /'
        ERRORS=$((ERRORS + 1))
    fi

    # A script that does not even parse under the invoking bash is broken on
    # this platform. On macOS stock /bin/bash (3.2.57) this catches bash-3.2
    # parse failures — e.g. double quotes nested inside a double-quoted `$(...)`
    # (Issue #9461) — that bash 4+ tolerates, so CI (Linux bash 4) stays green
    # while the local run is red. Report the parser diagnostic.
    if ! parse_err=$(bash -n "$f" 2>&1); then
        echo "ERROR: $f does not parse under $(bash --version | head -1):"
        printf '%s\n' "$parse_err" | sed 's/^/    /'
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
