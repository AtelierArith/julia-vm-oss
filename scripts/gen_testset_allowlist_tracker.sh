#!/usr/bin/env bash
# gen_testset_allowlist_tracker.sh — generate the Issue #9472 umbrella-tracker
# body from docs/vm/TESTSET_FAILURE_ALLOWLIST.tsv (Issue #9698).
#
# The TSV plus the two-sided ratchet in
# subset_julia_vm/tests/fixture_tests.rs (testset_gate_verdict) are the
# EXECUTABLE source of truth for the broken-but-green fixture backlog. The
# GitHub Issue #9472 body is a DERIVED snapshot: it must never be hand-edited
# into a divergent state. After ANY change to the TSV (row removed because the
# bug is fixed, or — exceptionally — a row added with a `bug` Issue reference),
# regenerate and update the Issue body:
#
#   bash scripts/gen_testset_allowlist_tracker.sh > /tmp/tracker_9472.md
#   gh issue edit 9472 --repo AtelierArith/ailujsoi --body-file /tmp/tracker_9472.md
#
# Output: a markdown document (stdout) containing
#   * a generated-content notice declaring the TSV the source of truth,
#   * active-row counts (total, by classification, by fixture category),
#   * a per-row checklist grouped by classification,
#   * the sha256 of the TSV the snapshot was generated from.
# The output is deterministic (no timestamps; LC_ALL=C sort ordering), so
# running the script twice on the same TSV yields byte-identical documents.
#
# Verification mode (optional): when SJULIA_TESTSET_GATE_LOG points at the log
# produced by a full `SJULIA_TESTSET_GATE_LOG=<path> cargo nextest run
# --release --test fixture_tests` sweep (one manifest `file` path per failing
# fixture), the script cross-checks the log's failing-fixture set against the
# TSV rows and exits 1 on any mismatch, printing the diff to stderr. This is
# the same two-sided comparison the ratchet enforces per-fixture at test time.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TSV="$REPO_ROOT/docs/vm/TESTSET_FAILURE_ALLOWLIST.tsv"

if [[ ! -f "$TSV" ]]; then
    echo "error: $TSV not found" >&2
    exit 1
fi

export LC_ALL=C

# Active rows: file<TAB>class<TAB>issue<TAB>reason (comments/blank lines skipped).
# Sorted for deterministic output regardless of TSV row order.
ROWS="$(awk -F'\t' '
    /^[[:space:]]*#/ { next }
    /^[[:space:]]*$/ { next }
    {
        if (NF < 4) {
            printf "error: malformed allowlist row (need 4 tab-separated columns): %s\n", $0 > "/dev/stderr"
            exit 1
        }
        if ($2 != "sjulia-bug" && $2 != "bad-fixture") {
            printf "error: unknown classification %s in row: %s\n", $2, $0 > "/dev/stderr"
            exit 1
        }
        print
    }
' "$TSV" | sort)"

TOTAL=0
if [[ -n "$ROWS" ]]; then
    TOTAL="$(printf '%s\n' "$ROWS" | wc -l | tr -d ' ')"
fi

# --- Optional gate-log cross-check (Issue #9698) --------------------------
if [[ -n "${SJULIA_TESTSET_GATE_LOG:-}" ]]; then
    LOG="$SJULIA_TESTSET_GATE_LOG"
    if [[ ! -f "$LOG" ]]; then
        echo "error: SJULIA_TESTSET_GATE_LOG=$LOG does not exist" >&2
        exit 1
    fi
    TSV_FILES="$(printf '%s\n' "$ROWS" | awk -F'\t' 'NF {print $1}' | sort -u)"
    LOG_FILES="$(sort -u "$LOG")"
    if [[ "$TSV_FILES" != "$LOG_FILES" ]]; then
        echo "error: TSV allowlist and $LOG disagree (Issue #9698 sync check):" >&2
        echo "--- only in TSV (stale rows — fixtures no longer fail):" >&2
        comm -23 <(printf '%s\n' "$TSV_FILES") <(printf '%s\n' "$LOG_FILES") | sed 's/^/  /' >&2
        echo "--- only in gate log (new broken-but-green fixtures):" >&2
        comm -13 <(printf '%s\n' "$TSV_FILES") <(printf '%s\n' "$LOG_FILES") | sed 's/^/  /' >&2
        exit 1
    fi
    echo "gate-log check OK: $TOTAL TSV row(s) match $LOG" >&2
fi

# --- Markdown snapshot -----------------------------------------------------
TSV_SHA="$(sha256sum "$TSV" | awk '{print $1}')"

count_class() { # count_class <classification>
    if [[ -z "$ROWS" ]]; then echo 0; else
        printf '%s\n' "$ROWS" | awk -F'\t' -v c="$1" '$2 == c' | wc -l | tr -d ' '
    fi
}
SJULIA_BUG_COUNT="$(count_class sjulia-bug)"
BAD_FIXTURE_COUNT="$(count_class bad-fixture)"

cat <<EOF
> [!IMPORTANT]
> **This Issue body is GENERATED — do not hand-edit.** The source of truth is
> \`docs/vm/TESTSET_FAILURE_ALLOWLIST.tsv\` plus the two-sided ratchet in
> \`subset_julia_vm/tests/fixture_tests.rs\` (\`testset_gate_verdict\`). After any
> TSV change, regenerate this body with
> \`bash scripts/gen_testset_allowlist_tracker.sh\` and update it via
> \`gh issue edit 9472 --body-file <generated.md>\` (sync mechanism: Issue #9698).
> The original hand-written 2026-07-06 grandfathering snapshot (101 fixtures)
> is preserved in this Issue's edit history.

## Umbrella tracker: broken-but-green fixture backlog (Issue #9360 gate)

A **broken-but-green** fixture runs a \`@testset\`/\`@test\` that FAILS yet still
ends with a value matching its manifest \`expected\`, so the pre-#9360 harness
passed it while a direct \`sjulia <fixture>\` CLI run printed \`Test Failed\` and
exited non-zero. Such fixtures are grandfathered in the TSV; the gate is a
two-sided ratchet (no NEW broken-but-green fixture can land; a row whose
fixture starts passing fails the suite as stale, forcing removal).

Verify any row with:

\`\`\`
cd subset_julia_vm && ./target/release/sjulia tests/fixtures/<file>
julia --startup-file=no tests/fixtures/<file>
\`\`\`

Classification: \`sjulia-bug\` = upstream julia passes, sjulia records a
\`@test\` failure (real VM bug); \`bad-fixture\` = upstream julia ALSO fails
(wrong/non-portable assertion — fix or remove the fixture).

## Current snapshot

| Metric | Count |
|---|---|
| **Active allowlist rows (total)** | **$TOTAL** |
| \`sjulia-bug\` | $SJULIA_BUG_COUNT |
| \`bad-fixture\` | $BAD_FIXTURE_COUNT |

TSV sha256: \`$TSV_SHA\`
EOF

if [[ "$TOTAL" -eq 0 ]]; then
    cat <<'EOF'

**The backlog is CLEARED — the allowlist contains no active rows.** Every
grandfathered fixture has been fixed (or corrected/removed as a bad fixture)
and its row deleted by the ratchet. The gate stays armed: any registered
fixture that records a `@test` failure now fails the suite immediately.
EOF
else
    echo
    echo "### Rows by fixture category"
    echo
    echo "| Category | Rows |"
    echo "|---|---|"
    printf '%s\n' "$ROWS" | awk -F'\t' '{split($1, p, "/"); print p[1]}' \
        | sort | uniq -c | sort -k2 \
        | awk '{printf "| `%s` | %s |\n", $2, $1}'
    for class in sjulia-bug bad-fixture; do
        CLASS_ROWS="$(printf '%s\n' "$ROWS" | awk -F'\t' -v c="$class" '$2 == c')"
        [[ -z "$CLASS_ROWS" ]] && continue
        echo
        case "$class" in
            sjulia-bug) echo "## sjulia-bug — julia passes, sjulia fails" ;;
            bad-fixture) echo "## bad-fixture — julia also fails (fix or remove the fixture)" ;;
        esac
        echo
        printf '%s\n' "$CLASS_ROWS" | awk -F'\t' '{
            issue = $3
            sub(/^#/, "", issue)
            printf "- [ ] `%s` — #%s: %s\n", $1, issue, $4
        }'
    done
fi
