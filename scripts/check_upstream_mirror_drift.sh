#!/usr/bin/env bash
# check_upstream_mirror_drift.sh
#
# Detect semantic drift between mirrored Pure Julia files in
# subset_julia_vm/src/julia/ and their upstream counterparts in the
# julia/ submodule.
#
# Each mirrored file carries a machine-readable header of the form:
#
#   # upstream: julia/base/<path> @ <commit-sha> (swept YYYY-MM-DD)
#
# This script reads every such header, then runs
#   git -C julia diff <recorded-commit>..HEAD -- <upstream-path>
# to check whether the upstream file changed since the sweep.  A
# non-empty diff means the sjulia mirror may have drifted and needs
# human triage.
#
# Exit behaviour: this script ALWAYS exits 0 (report only, no hard
# fail) because drift is expected whenever the julia/ submodule is
# bumped and requires human review — not automatic blocking.  Wire
# the step as informational in CI so developers see the report
# without blocking merges (Issue #9005).
#
# Usage (run from the repository root):
#   bash scripts/check_upstream_mirror_drift.sh
#
# On a submodule bump: run this script, review the diff output for
# each drifted file, and update the swept header once re-triaged.
# See docs/vm/CHECKLISTS.md "julia/ サブモジュール更新" for the
# full submodule-bump triage workflow.

set -euo pipefail

SRC_DIR="subset_julia_vm/src/julia"
JULIA_DIR="julia"

if [[ ! -d "$SRC_DIR" ]]; then
    echo "ERROR: $SRC_DIR not found. Run from the repository root."
    exit 1
fi
if [[ ! -d "$JULIA_DIR/.git" ]] && [[ ! -f "$JULIA_DIR/.git" ]]; then
    echo "WARNING: julia/ submodule is not initialised — skipping drift check."
    echo "  Run: git submodule update --init julia"
    exit 0
fi

CURRENT_SHA=$(git -C "$JULIA_DIR" rev-parse HEAD 2>/dev/null || true)
if [[ -z "$CURRENT_SHA" ]]; then
    echo "WARNING: Could not determine julia/ submodule HEAD — skipping."
    exit 0
fi

drifted=()
ok=()
unknown=()

# Collect all mirrored files by scanning for the upstream: header.
mirrored_files=()
while IFS= read -r f; do
    mirrored_files+=("$f")
done < <(grep -rl '^# upstream: julia/' "$SRC_DIR" 2>/dev/null | sort)

if [[ ${#mirrored_files[@]} -eq 0 ]]; then
    echo "OK: no mirrored files with upstream: headers found in $SRC_DIR."
    exit 0
fi

for sjulia_file in "${mirrored_files[@]+"${mirrored_files[@]}"}"; do
    # Extract the header line:
    #   # upstream: julia/base/foo.jl @ <sha> (swept YYYY-MM-DD)
    header=$(grep -m1 '^# upstream: julia/' "$sjulia_file" 2>/dev/null || true)
    if [[ -z "$header" ]]; then
        continue
    fi

    # Parse upstream path and recorded commit SHA.
    # Header format: # upstream: <upstream-path> @ <sha> (swept <date>)
    # Fields:        1 2          3                4 5     6
    upstream_path=$(echo "$header" | awk '{print $3}')
    recorded_sha=$(echo "$header"  | awk '{print $5}')

    if [[ -z "$upstream_path" ]] || [[ -z "$recorded_sha" ]]; then
        echo "WARNING: malformed upstream: header in $sjulia_file — skipping"
        echo "  Header: $header"
        unknown+=("$sjulia_file")
        continue
    fi

    # Strip leading "julia/" prefix to get the path relative to the submodule.
    rel_path="${upstream_path#julia/}"

    # Validate that the recorded SHA exists in the submodule.
    if ! git -C "$JULIA_DIR" cat-file -e "${recorded_sha}^{commit}" 2>/dev/null; then
        echo "WARNING: recorded sweep commit $recorded_sha not found in julia/ for $sjulia_file"
        unknown+=("$sjulia_file")
        continue
    fi

    # Skip if submodule HEAD equals the recorded SHA (no bump happened).
    if [[ "$CURRENT_SHA" = "$recorded_sha" ]]; then
        ok+=("$sjulia_file")
        continue
    fi

    # Check whether the upstream file changed between recorded sweep and now.
    diff_output=$(git -C "$JULIA_DIR" diff "${recorded_sha}..HEAD" -- "$rel_path" 2>/dev/null || true)
    if [[ -n "$diff_output" ]]; then
        drifted+=("$sjulia_file upstream:$upstream_path recorded:$recorded_sha")
    else
        ok+=("$sjulia_file")
    fi
done

echo "============================================================"
echo "Upstream mirror drift report (Issue #9005)"
echo "julia/ submodule HEAD: $CURRENT_SHA"
echo "Mirrored files with upstream: header: ${#mirrored_files[@]}"
echo "============================================================"

if [[ ${#ok[@]} -gt 0 ]]; then
    echo ""
    echo "OK (no upstream change since sweep): ${#ok[@]} file(s)"
    for f in "${ok[@]+"${ok[@]}"}"; do
        echo "  [ok] $f"
    done
fi

if [[ ${#unknown[@]} -gt 0 ]]; then
    echo ""
    echo "UNKNOWN (malformed header or missing commit): ${#unknown[@]} file(s)"
    for f in "${unknown[@]+"${unknown[@]}"}"; do
        echo "  [?]  $f"
    done
fi

if [[ ${#drifted[@]} -gt 0 ]]; then
    echo ""
    echo "DRIFTED (upstream changed since last sweep — needs triage): ${#drifted[@]} file(s)"
    for entry in "${drifted[@]+"${drifted[@]}"}"; do
        sjulia_file="${entry%% upstream:*}"
        rest="${entry#* upstream:}"
        up_path="${rest%% recorded:*}"
        rec_sha="${rest#* recorded:}"
        echo "  [drift] $sjulia_file"
        echo "          upstream: $up_path"
        echo "          swept at: $rec_sha"
        echo "          diff: git -C julia diff ${rec_sha}..HEAD -- ${up_path#julia/}"
    done
    echo ""
    echo "Action: for each drifted file, review the upstream diff, update the"
    echo "  sjulia mirror as needed, and bump the swept commit + date in the"
    echo "  '# upstream: ...' header once re-triaged."
    echo "  See docs/vm/CHECKLISTS.md 'julia/ サブモジュール更新' for the"
    echo "  full triage workflow."
    echo ""
    echo "NOTE: this script exits 0 (informational) — drift is expected on"
    echo "  submodule bumps and requires human review, not automatic blocking."
fi

echo "============================================================"
echo "Drift check complete. Exit 0 (report only — no hard fail)."
exit 0
