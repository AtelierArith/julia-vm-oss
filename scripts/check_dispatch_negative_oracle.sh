#!/usr/bin/env bash
# check_dispatch_negative_oracle.sh
#
# Ratchet the dispatch parity corpus so loose-match prevention keeps covering
# upstream MethodError negative cases (Issue #9567).

set -euo pipefail

CORPUS="${1:-subset_julia_vm/tests/fixtures/dispatch_parity/corpus.toml}"
MIN_METHOD_ERROR_CASES=7

REQUIRED_CASES="
neg_vector_invariance_methoderror_9567
neg_abstract_function_not_matrix_9567
neg_diagonal_mixed_methoderror_9567
neg_keyword_no_match_methoderror_9567
"

case_block() {
    local case_name="$1"
    awk -v case_name="$case_name" '
        /^\[\[groups\.cases\]\]/ {
            if (in_block && block ~ "name = \"" case_name "\"") {
                print block
                found = 1
            }
            in_block = 1
            block = $0 "\n"
            next
        }
        /^\[\[groups\]\]/ {
            if (in_block && block ~ "name = \"" case_name "\"") {
                print block
                found = 1
            }
            in_block = 0
            block = ""
            next
        }
        in_block {
            block = block $0 "\n"
        }
        END {
            if (in_block && !found && block ~ "name = \"" case_name "\"") {
                print block
            }
        }
    ' "$CORPUS"
}

check_corpus() {
    if [[ ! -f "$CORPUS" ]]; then
        echo "ERROR: dispatch parity corpus not found: $CORPUS"
        exit 1
    fi

    local errors=0
    local count
    count=$(grep -c '^expected = "MethodError"$' "$CORPUS" || true)
    if [[ "$count" -lt "$MIN_METHOD_ERROR_CASES" ]]; then
        echo "ERROR: dispatch negative oracle MethodError case count regressed: $count < $MIN_METHOD_ERROR_CASES (Issue #9567)"
        errors=$((errors + 1))
    fi

    local case_name block
    for case_name in $REQUIRED_CASES; do
        block="$(case_block "$case_name")"
        if [[ -z "$block" ]]; then
            echo "ERROR: missing required negative oracle case '$case_name' (Issue #9567)"
            errors=$((errors + 1))
            continue
        fi
        if ! printf '%s\n' "$block" | grep -q '^expected = "MethodError"$'; then
            echo "ERROR: negative oracle case '$case_name' must record expected = \"MethodError\" (Issue #9567)"
            errors=$((errors + 1))
        fi
        if printf '%s\n' "$block" | grep -q '^allow_mismatch = '; then
            echo "ERROR: negative oracle case '$case_name' must not be allowlisted (Issue #9567)"
            errors=$((errors + 1))
        fi
    done

    if [[ "$errors" -gt 0 ]]; then
        exit 1
    fi

    echo "OK: dispatch negative oracle covers $count MethodError cases and required #9567 cells."
}

check_corpus
