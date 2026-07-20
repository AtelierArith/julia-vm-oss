#!/usr/bin/env bash
# Replay a fixture journal in one Rust test process and minimize the prefix.
# Issue #8709, parent #8687.

set -euo pipefail
cd "$(dirname "$0")/.."

usage() {
  cat <<'USAGE'
Usage:
  scripts/replay_fixture_sequence.sh [options] <journal.jsonl> <failing_fixture_or_test>

Options:
  --cache <clean|reuse|disabled>  Cache mode for each replay (default: clean).
                                  clean: remove persistent cache files before each replay.
                                  reuse: leave cache state untouched.
                                  disabled: remove caches and disable compile/persistent caches.
  --out-dir <dir>                 Output directory (default: target/fixture-replay-8709).
  --plan-only                     Parse the journal and print the initial replay plan only.
  --help                          Show this help.

The journal is the JSONL file produced by SJULIA_FIXTURE_JOURNAL. Each replay
candidate is executed by fixture_sequence_replay_8709_from_env in one Rust test
process via SJULIA_FIXTURE_SEQUENCE_FILE.
USAGE
}

cache_mode="clean"
out_dir="target/fixture-replay-8709"
plan_only=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --cache)
      [[ $# -ge 2 ]] || { echo "ERROR: --cache needs a value" >&2; exit 2; }
      cache_mode="$2"
      shift 2
      ;;
    --out-dir)
      [[ $# -ge 2 ]] || { echo "ERROR: --out-dir needs a value" >&2; exit 2; }
      out_dir="$2"
      shift 2
      ;;
    --plan-only)
      plan_only=1
      shift
      ;;
    --help|-h)
      usage
      exit 0
      ;;
    --*)
      echo "ERROR: unknown option: $1" >&2
      usage >&2
      exit 2
      ;;
    *)
      break
      ;;
  esac
done

[[ $# -eq 2 ]] || { usage >&2; exit 2; }

journal="$1"
failing_selector="$2"

case "$cache_mode" in
  clean|reuse|disabled) ;;
  *) echo "ERROR: --cache must be clean, reuse, or disabled" >&2; exit 2 ;;
esac

[[ -f "$journal" ]] || { echo "ERROR: journal not found: $journal" >&2; exit 1; }

mkdir -p "$out_dir"
prefix_file="$out_dir/prefix.txt"
failing_file="$out_dir/failing.txt"
candidate_file="$out_dir/candidate_sequence.txt"
template_file="$out_dir/fixture_sequence_regression_template.rs"

python3 - "$journal" "$failing_selector" "$prefix_file" "$failing_file" <<'PY'
import json
import os
import sys

journal, failing_selector, prefix_file, failing_file = sys.argv[1:5]

def selector_for(entry):
    return entry.get("fixture") or entry.get("test") or entry.get("fixture_path")

def matches(entry, wanted):
    values = [
        entry.get("test"),
        entry.get("fixture"),
        entry.get("fixture_path"),
    ]
    norm_wanted = os.path.normpath(wanted)
    for value in values:
        if not value:
            continue
        if value == wanted or os.path.normpath(value) == norm_wanted:
            return True
        if entry.get("fixture_path") == value and os.path.basename(value) == wanted:
            return True
    return False

prefix = []
failing = None
with open(journal, encoding="utf-8") as fh:
    for lineno, raw in enumerate(fh, 1):
        line = raw.strip()
        if not line:
            continue
        try:
            entry = json.loads(line)
        except json.JSONDecodeError as exc:
            raise SystemExit(f"malformed journal JSON on line {lineno}: {exc}") from exc
        selector = selector_for(entry)
        if not selector:
            raise SystemExit(f"journal line {lineno} lacks fixture/test selector")
        if matches(entry, failing_selector):
            failing = selector
            break
        prefix.append(selector)

if failing is None:
    raise SystemExit(f"failing fixture/test not found in journal: {failing_selector}")

with open(prefix_file, "w", encoding="utf-8") as fh:
    for item in prefix:
        print(item, file=fh)
with open(failing_file, "w", encoding="utf-8") as fh:
    print(failing, file=fh)

print(f"failing_fixture={failing}")
print(f"predecessor_count={len(prefix)}")
print(f"candidate_count={len(prefix) + 1}")
print("runner_test=fixture_sequence_replay_8709_from_env")
PY

if [[ "$plan_only" -eq 1 ]]; then
  exit 0
fi

prefix_count="$(wc -l < "$prefix_file" | tr -d ' ')"
failing_fixture="$(cat "$failing_file")"

write_candidate() {
  local count="$1"
  : > "$candidate_file"
  if [[ "$count" -gt 0 ]]; then
    sed -n "1,${count}p" "$prefix_file" >> "$candidate_file"
  fi
  printf '%s\n' "$failing_fixture" >> "$candidate_file"
}

prepare_cache() {
  case "$cache_mode" in
    reuse)
      ;;
    clean)
      rm -f target/sjulia_base_cache_*.bin target/sjulia_prelude_program_*.bin
      rm -rf "${TMPDIR:-/tmp}/subset_julia_vm_cache"
      unset SUBSET_JULIA_VM_DISABLE_CACHE
      unset SUBSET_JULIA_VM_DISABLE_PERSISTENT_BASE_CACHE
      unset SUBSET_JULIA_VM_DISABLE_PERSISTENT_PRELUDE_CACHE
      ;;
    disabled)
      rm -f target/sjulia_base_cache_*.bin target/sjulia_prelude_program_*.bin
      rm -rf "${TMPDIR:-/tmp}/subset_julia_vm_cache"
      export SUBSET_JULIA_VM_DISABLE_CACHE=1
      export SUBSET_JULIA_VM_DISABLE_PERSISTENT_BASE_CACHE=1
      export SUBSET_JULIA_VM_DISABLE_PERSISTENT_PRELUDE_CACHE=1
      ;;
  esac
}

run_candidate() {
  local count="$1"
  local log="$out_dir/replay_${count}.log"
  write_candidate "$count"
  prepare_cache
  echo "replay: prefix=$count total=$((count + 1)) cache=$cache_mode log=$log"
  if SJULIA_FIXTURE_SEQUENCE_FILE="$candidate_file" \
      cargo nextest run --release --test fixture_tests \
        fixture_sequence_replay_8709_from_env --no-fail-fast >"$log" 2>&1; then
    return 1
  fi
  return 0
}

if ! run_candidate "$prefix_count"; then
  echo "ERROR: full journal prefix + failing fixture did not reproduce failure" >&2
  echo "  sequence: $candidate_file" >&2
  echo "  log: $out_dir/replay_${prefix_count}.log" >&2
  exit 1
fi

lo=0
hi="$prefix_count"
while [[ "$lo" -lt "$hi" ]]; do
  mid=$(((lo + hi) / 2))
  if run_candidate "$mid"; then
    hi="$mid"
  else
    lo=$((mid + 1))
  fi
done

write_candidate "$lo"

{
  echo "// Paste into subset_julia_vm/tests/fixture_tests.rs near fixture_sequence_replay_8709_from_env."
  echo "// Generated by scripts/replay_fixture_sequence.sh for Issue #8709."
  echo "#[test]"
  echo "fn fixture_sequence_regression_issue_NNNN() {"
  echo "    let selectors = vec!["
  sed 's/\\/\\\\/g; s/"/\\"/g; s/^/        "/; s/$/".to_string(),/' "$candidate_file"
  echo "    ];"
  echo "    let result = std::thread::Builder::new()"
  echo "        .stack_size(FIXTURE_TEST_STACK_SIZE)"
  echo "        .spawn(move || {"
  echo "            let manifest = load_manifest();"
  echo "            run_test_cases_by_selector(&manifest, &selectors);"
  echo "        })"
  echo "        .expect(\"Failed to spawn fixture sequence regression thread\")"
  echo "        .join();"
  echo "    if let Err(e) = result {"
  echo "        std::panic::resume_unwind(e);"
  echo "    }"
  echo "}"
} > "$template_file"

echo "minimal_predecessor_count=$lo"
echo "minimal_sequence=$candidate_file"
echo "regression_template=$template_file"
