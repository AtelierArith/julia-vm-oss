#!/usr/bin/env bash
# check_aot_gate_selection.sh — canonical AoT path selection (Issue #10866).

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
SELECTOR="$REPO_ROOT/scripts/select_aot_gate.py"
CONFIG="$REPO_ROOT/.github/aot-gate-paths.txt"
PR_FAST="$REPO_ROOT/.github/workflows/pr-fast.yml"
CI="$REPO_ROOT/.github/workflows/ci.yml"
TEST="$REPO_ROOT/tests/test_aot_gate_selection.py"

for required in "$SELECTOR" "$CONFIG" "$PR_FAST" "$CI" "$TEST"; do
  if [ ! -f "$required" ]; then
    echo "FAIL: AoT gate selection input is missing: $required (Issue #10866)." >&2
    exit 2
  fi
done

tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT INT TERM
status=0

selection_for() {
  local changed_path="$1" output selector_output
  printf '%s\n' "$changed_path" > "$tmp_dir/changed-files.txt"
  selector_output="$tmp_dir/selector-output.txt"
  if ! (cd "$REPO_ROOT" && python3 scripts/select_aot_gate.py \
      --changed-files "$tmp_dir/changed-files.txt" \
      --config .github/aot-gate-paths.txt > "$selector_output" 2>&1); then
    output="$(cat "$selector_output")"
    echo "FAIL: AoT gate selector could not evaluate '$changed_path':" >&2
    printf '%s\n' "$output" | sed 's/^/  /' >&2
    return 2
  fi
  output="$(cat "$selector_output")"
  printf '%s\n' "$output" | sed -n 's/^aot=//p' | tail -n 1
}

expect_selection() {
  local expected="$1" changed_path="$2" failure="$3" actual
  actual="$(selection_for "$changed_path")"
  if [ "$actual" != "$expected" ]; then
    echo "FAIL: $failure (Issue #10866)." >&2
    echo "  path: $changed_path" >&2
    echo "  expected aot=$expected, got aot=${actual:-<missing>}" >&2
    status=1
  fi
}

expect_selection true \
  "subset_julia_vm_types/src/inference_core/type_core.rs" \
  "shared inference-core changes no longer select the AoT gate"
expect_selection true \
  "subset_julia_vm/src/aot/types.rs" \
  "direct AoT changes no longer select the AoT gate"
expect_selection true \
  "subset_julia_vm/src/bin/aot.rs" \
  "legacy AoT entry-point changes no longer select the AoT gate"
expect_selection false \
  "docs/vm/TESTING.md" \
  "docs-only changes unexpectedly select the AoT gate"
expect_selection false \
  "SubsetJuliaVMApp/Views/EditorView.swift" \
  "unrelated application changes unexpectedly select the AoT gate"

delegation="scripts/select_aot_gate.py --changed-files changed-files.txt --github-output \"\$GITHUB_OUTPUT\""
for workflow in "$PR_FAST" "$CI"; do
  count="$(grep -Fc -- "$delegation" "$workflow")"
  if [ "$count" -ne 1 ]; then
    relative="${workflow#"$REPO_ROOT/"}"
    echo "FAIL: $relative must delegate AoT path selection exactly once to scripts/select_aot_gate.py (Issue #10866)." >&2
    status=1
  fi
  if grep -Ei 'grep .*aot' "$workflow" >/dev/null; then
    relative="${workflow#"$REPO_ROOT/"}"
    echo "FAIL: $relative reintroduced an inline AoT path matcher; edit .github/aot-gate-paths.txt instead (Issue #10866)." >&2
    status=1
  fi
done

expect_literal_once() {
  local file="$1" literal="$2" failure="$3" count relative
  count="$(grep -Fc -- "$literal" "$file")"
  if [ "$count" -ne 1 ]; then
    relative="${file#"$REPO_ROOT/"}"
    echo "FAIL: $relative $failure (Issue #10866)." >&2
    status=1
  fi
}

expect_step_condition() {
  local file="$1" step_name="$2" condition="$3" failure="$4" count relative
  if ! count="$(awk -v step="      - name: $step_name" -v guard="        if: $condition" '
    $0 == step {
      seen++
      if ((getline next_line) > 0 && next_line == guard) matched++
    }
    END { print (seen == 1 && matched == 1 ? 1 : 0) }
  ' "$file")"; then
    relative="${file#"$REPO_ROOT/"}"
    echo "FAIL: could not inspect $relative step conditions (Issue #10866)." >&2
    status=1
    return
  fi
  if [ "$count" -ne 1 ]; then
    relative="${file#"$REPO_ROOT/"}"
    echo "FAIL: $relative $failure (Issue #10866)." >&2
    status=1
  fi
}

expect_literal_once "$PR_FAST" \
  "aot: \${{ steps.detect.outputs.aot }}" \
  "must project the selector's aot output from the changes job exactly once"
expect_literal_once "$PR_FAST" \
  "if: needs.changes.outputs.aot == 'true'" \
  "must gate the AoT job on the changes job's aot output exactly once"
expect_step_condition "$CI" "Run AoT gate" \
  "steps.changes.outputs.aot == 'true'" \
  "must run the AoT gate when the selector returns true"
expect_step_condition "$CI" "Skip AoT gate" \
  "steps.changes.outputs.aot != 'true'" \
  "must only skip the AoT gate when the selector does not return true"

if ! (cd "$REPO_ROOT" && python3 -m unittest tests/test_aot_gate_selection.py); then
  echo "FAIL: executable AoT gate selection regression tests failed (Issue #10866)." >&2
  status=1
fi

if [ "$status" -eq 0 ]; then
  echo "OK: both workflows share and consume the canonical AoT selector; compatibility, shared-inference, and negative path controls pass (Issue #10866)."
fi

exit "$status"
