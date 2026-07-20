#!/usr/bin/env bash
# List project memory entries that reference closed GitHub Issues.
#
# This is a triage helper only: it never edits memory files. Humans decide
# whether to promote technical knowledge into docs/vm/, shrink an entry to a
# pointer, or delete pure work logs.
#
# Usage:
#   bash scripts/memory_triage.sh
#   bash scripts/memory_triage.sh --repo OWNER/REPO

set -euo pipefail

usage() {
    cat <<'USAGE'
Usage: bash scripts/memory_triage.sh [--repo OWNER/REPO]

Scans memory/project/*.md (active entries; memory/archive/ is excluded), extracts GitHub #NNNN references from frontmatter
and body text, queries `gh issue view --json state,url`, and lists entries that
reference at least one closed Issue.

Output columns:
  status         READY when all referenced Issues are closed, REVIEW when some
                 referenced Issues remain open or could not be read.
  file           memory/project/*.md entry to triage.
  closed_issues  Closed issue refs found in the entry.
  open_issues    Open issue refs found in the entry.
  pr_refs        Pull request refs found while resolving #NNNN references.
  unreadable     Refs gh could not resolve.
USAGE
}

REPO=""
while [[ "$#" -gt 0 ]]; do
    case "$1" in
        --repo)
            if [[ "$#" -lt 2 ]]; then
                echo "ERROR: --repo requires OWNER/REPO." >&2
                exit 1
            fi
            REPO="$2"
            shift 2
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            echo "ERROR: unknown argument: $1" >&2
            usage >&2
            exit 1
            ;;
    esac
done

if [[ ! -d memory ]]; then
    echo "ERROR: memory/ not found. Run this script from the repository root." >&2
    exit 1
fi

if ! command -v gh >/dev/null 2>&1; then
    echo "ERROR: gh is required to query Issue state." >&2
    exit 1
fi

if [[ -z "$REPO" ]]; then
    REPO=$(gh repo view --json nameWithOwner --jq '.nameWithOwner' 2>/dev/null || true)
fi

if [[ -z "$REPO" ]]; then
    echo "ERROR: could not infer repository. Pass --repo OWNER/REPO." >&2
    exit 1
fi

TMPDIR=${TMPDIR:-/tmp}
WORKDIR=$(mktemp -d "$TMPDIR/memory_triage.XXXXXX")
trap 'rm -rf "$WORKDIR"' EXIT

append_ref() {
    local current="$1"
    local ref="$2"
    if [[ -z "$current" ]]; then
        printf '%s' "$ref"
    else
        printf '%s %s' "$current" "$ref"
    fi
}

issue_info() {
    local number="$1"
    local cache="$WORKDIR/issue_$number.tsv"

    if [[ ! -f "$cache" ]]; then
        if ! gh issue view "$number" --repo "$REPO" --json state,url --jq '[.state, .url] | @tsv' >"$cache" 2>"$cache.err"; then
            rm -f "$cache"
            printf 'UNREADABLE\t\t\n'
            return 0
        fi
    fi

    local state
    local url
    IFS=$'\t' read -r state url < "$cache"

    if [[ "$url" == *"/pull/"* ]]; then
        printf 'PR\t%s\t%s\n' "$state" "$url"
    elif [[ "$url" == *"/issues/"* ]]; then
        printf 'ISSUE\t%s\t%s\n' "$state" "$url"
    else
        printf 'UNREADABLE\t%s\t%s\n' "$state" "$url"
    fi
}

memory_lines=0
if [[ -f memory/MEMORY.md ]]; then
    memory_lines=$(wc -l < memory/MEMORY.md | tr -d ' ')
fi

project_count=0
candidate_count=0
ready_count=0
review_count=0

printf '# memory triage for %s\n' "$REPO"
printf '# MEMORY.md lines: %s (threshold: 200)\n' "$memory_lines"
printf '# Generated: %s\n' "$(date -u '+%Y-%m-%dT%H:%M:%SZ')"
printf 'status\tfile\tclosed_issues\topen_issues\tpr_refs\tunreadable\n'

for file in memory/project/*.md; do
    [[ -f "$file" ]] || continue
    project_count=$((project_count + 1))

    refs_file="$WORKDIR/refs_$(basename "$file").txt"
    grep -Eoh '#[0-9]+|github\.com/[^[:space:])>]+/(issues|pull)/[0-9]+' "$file" \
        | sed -E 's/^#//; s#.*/(issues|pull)/##' \
        | sort -n -u > "$refs_file" || true

    if [[ ! -s "$refs_file" ]]; then
        continue
    fi

    closed_issues=""
    open_issues=""
    pr_refs=""
    unreadable=""

    while IFS= read -r number; do
        [[ -n "$number" ]] || continue
        info=$(issue_info "$number")
        kind=$(printf '%s' "$info" | awk -F '\t' '{print $1}')
        state=$(printf '%s' "$info" | awk -F '\t' '{print $2}')

        if [[ "$kind" == "ISSUE" && "$state" == "CLOSED" ]]; then
            closed_issues=$(append_ref "$closed_issues" "#$number")
        elif [[ "$kind" == "ISSUE" && "$state" == "OPEN" ]]; then
            open_issues=$(append_ref "$open_issues" "#$number")
        elif [[ "$kind" == "PR" ]]; then
            pr_refs=$(append_ref "$pr_refs" "#$number:$state")
        else
            unreadable=$(append_ref "$unreadable" "#$number")
        fi
    done < "$refs_file"

    if [[ -z "$closed_issues" ]]; then
        continue
    fi

    candidate_count=$((candidate_count + 1))
    status="READY"
    if [[ -n "$open_issues" || -n "$unreadable" ]]; then
        status="REVIEW"
        review_count=$((review_count + 1))
    else
        ready_count=$((ready_count + 1))
    fi

    printf '%s\t%s\t%s\t%s\t%s\t%s\n' \
        "$status" \
        "$file" \
        "${closed_issues:-}" \
        "${open_issues:-}" \
        "${pr_refs:-}" \
        "${unreadable:-}"
done

printf '# Summary: project_files=%s candidates=%s ready=%s review=%s\n' \
    "$project_count" "$candidate_count" "$ready_count" "$review_count"
