#!/usr/bin/env bash
# audit_julia_base_stubs.sh
#
# Audit Pure Julia helpers under subset_julia_vm/src/julia/ for
# silently-wrong stub implementations.
#
# NOTE ON NAMING: this is intentionally NOT named check_*.sh. It predates
# the check_*.sh registration perimeter and remains an audit_* tool for
# compatibility with existing docs and local workflows. CI now shellchecks and
# runs it directly (Issue #8459); a future cleanup can decide whether to rename
# it to check_julia_base_stubs.sh.
#
# A "silent stub" is a `function NAME(args...)` whose ENTIRE body is a
# single trivial `return <constant-or-bare-arg>` and whose positional
# arguments are all UNTYPED. Such a body compiles as valid Julia and
# passes static analysis, yet can silently return the wrong value for
# every input it is supposed to discriminate on (the
# `isvarargtype(t) = false` / `unwrap_unionall(t) = t` shape behind
# Issues #4701 and #3909). Untyped + trivial-body is the fingerprint of
# a placeholder that nobody finished.
#
# Convention (Issue #4703): any such function that is *intentionally* a
# pass-through / constant (e.g. an optimizer hint the sjulia interpreter
# does not need, or a generic dispatch fallback that matches upstream)
# must carry a
#   # INTENTIONAL_NOOP (Issue #NNNN): <why a trivial body is correct>
# or, for a not-yet-finished placeholder that is known to be wrong,
#   # STUB(Issue #NNNN): <what is missing / which Issue tracks the fix>
# marker in the comment block immediately above the `function` keyword.
# The audit then surfaces every *unmarked* trivial untyped helper as a
# regression so silently-wrong stubs cannot creep back in.
#
# Past incidents this guards against:
#   - Issue #4701: isvarargtype / isvatuple were `return false` stubs
#   - PR #4693 (Issue #3909): unwrap_unionall was a `return t` stub
#   - PR #4695 (Issue #4694): rewrap_unionall was unimplementable
#   - Issue #5039: oneunit(x)=1 / imag(x)=0.0 untyped type-losing stubs
#
# Usage: run from the repository root
#   bash scripts/audit_julia_base_stubs.sh

set -euo pipefail

SRC_DIR="subset_julia_vm/src/julia"
if [[ ! -d "$SRC_DIR" ]]; then
    echo "ERROR: $SRC_DIR not found. Run from the repository root."
    exit 1
fi

# Files that have been swept against upstream julia/base and whose
# trivial-body untyped helpers are all either correct (and marked
# INTENTIONAL_NOOP) or tracked (and marked STUB). The list is now
# derived automatically from the per-file
#   # upstream: julia/base/<path> @ <commit> (swept YYYY-MM-DD)
# header convention (Issue #9005). Add the header to a file only after
# auditing every trivial untyped helper in it against julia/base; the
# script then guarantees no NEW unmarked silent stub is introduced into
# that file. Other src/julia files are deliberately excluded for now to
# avoid false positives on legitimate generic one-liners that have not
# been swept yet — extend coverage by adding the upstream: header.
AUDIT_FILES=()
while IFS= read -r candidate; do
    AUDIT_FILES+=("$candidate")
done < <(grep -rl '^# upstream: julia/' subset_julia_vm/src/julia/ 2>/dev/null | sort)

hits_file=$(mktemp)
trap 'rm -f "$hits_file"' EXIT

for file in "${AUDIT_FILES[@]}"; do
    [[ -f "$file" ]] || continue
    awk -v fname="$file" '
        function reset() { func_line = 0; func_text = "" }
        /^function / {
            func_line = NR
            func_text = $0
            argstart = index($0, "(")
            argend = 0
            for (i = length($0); i > argstart; i--) {
                if (substr($0, i, 1) == ")") { argend = i; break }
            }
            if (argstart == 0 || argend == 0) { reset(); comment_block = ""; next }
            args = substr($0, argstart + 1, argend - argstart - 1)
            split(args, parts, ";")
            positional = parts[1]
            untyped = (positional !~ /::/) && (positional !~ /^[[:space:]]*$/)
            if (func_text ~ /\)[[:space:]]+where[[:space:]]/) untyped = 0
            if (!untyped) { reset(); comment_block = ""; next }
            marker_ok = (comment_block ~ /(INTENTIONAL_NOOP|STUB)[[:space:]]*\(Issue[[:space:]]*#[0-9]+\)/)
            comment_block = ""
            next
        }
        /^[[:space:]]*#/ { comment_block = comment_block "\n" $0; next }
        /^[[:space:]]*$/ { comment_block = ""; next }
        func_line && NR == func_line + 1 && /^[[:space:]]*return / {
            body = $0
            sub(/^[[:space:]]*return[[:space:]]*/, "", body)
            sub(/[[:space:]]*$/, "", body)
            is_trivial = 0
            if (body == "nothing" || body == "true" || body == "false") is_trivial = 1
            else if (body ~ /^[A-Za-z_][A-Za-z_0-9]*$/) is_trivial = 1
            else if (body ~ /^-?[0-9]+(\.[0-9]+)?$/) is_trivial = 1
            else if (body ~ /^"[^"]*"$/) is_trivial = 1
            else if (body ~ /^[A-Za-z_][A-Za-z_0-9]*[[:space:]]*===[[:space:]]*[A-Za-z_][A-Za-z_0-9]*$/) is_trivial = 1
            if (is_trivial && !marker_ok) {
                printf("%s:%d: %s\n         body: return %s\n", fname, func_line, func_text, body)
            }
            reset()
            comment_block = ""
        }
        NR != func_line + 1 && func_line && !/^[[:space:]]*$/ {
            reset()
            comment_block = ""
        }
    ' "$file" >> "$hits_file"
done

hits=$(cat "$hits_file")
if [[ -n "$hits" ]]; then
    echo "ERROR: Unlabelled trivial-body untyped function(s) found in $SRC_DIR."
    echo "Each must be verified against upstream julia/base and then either"
    echo "replaced with a real implementation, or marked in the comment block"
    echo "immediately above the 'function' keyword with one of:"
    echo "  # INTENTIONAL_NOOP (Issue #NNNN): ...   (a trivial body is the correct semantics)"
    echo "  # STUB(Issue #NNNN): ...                (a placeholder tracked by an open Issue)"
    echo "See docs/vm/CODE_AUDITS.md (Pure Julia base/ silent-stub audit)."
    echo "$hits"
    exit 1
fi
echo "OK: untyped trivial-body Pure Julia helpers all carry an audit marker (Issue #4703)."
