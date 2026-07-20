#!/usr/bin/env bash
# check_dispatch_determinism.sh — dispatch-path HashMap/HashSet iteration ratchet
# (Issue #8659, parent #8641; inventory #8657, determinization #8658).
#
# Dispatch results must never depend on the seed-dependent iteration order of
# std `HashMap`/`HashSet` (#5966: promote-fallback recursion that only OOM'd in
# the full suite). The #8657 inventory classified every iteration site in the
# dispatch path and #8658 determinized the order-affecting ones; this ratchet
# keeps new hash-collection iterations from creeping back in.
#
# Counted patterns (whitespace-squashed, so rustfmt-split chains still match):
#   1. `method_tables.iter()/keys()/values()/into_iter()/drain()/retain()` —
#      iterating the name->MethodTable HashMap. Every remaining site must sort
#      (or otherwise determinize) before any first-match / candidate-order
#      consumption.
#   2. `.keys()` / `.values()` / `.values_mut()` on any receiver — these are
#      map-only accessors in the audited files.
# (A `method_tables.keys()/values()` match is counted once, not twice.)
#
# Every counted site is classified (harmless / diagnostics-only / determinized)
# in memory/project/project_8641_dispatch_hashmap_iteration_inventory.md.
# Adding a NEW iteration site in an audited file requires:
#   (a) proving order-independence per the inventory criteria (candidate
#       selection, tie-breaks, generated code order), or determinizing it
#       (definition-order Vec / IndexMap / explicit sort), and
#   (b) adding the classification to the inventory memory file and bumping the
#       baseline here in the same PR.
# When a refactor removes a site, lower the baseline (the script reminds you).

set -euo pipefail
cd "$(dirname "$0")/.."

# "<file>:<baseline>" pairs (bash 3.2 compatible — no associative arrays).
# inference_core moved to subset_julia_vm_types (crate split, milestone 60;
# old subset_julia_vm/src/inference_core/ paths are no longer valid)
BASELINES=(
  "subset_julia_vm_compile/src/compile/method_table.rs:0"
  "subset_julia_vm_types/src/inference_core/dispatch_resolver.rs:0"
  "subset_julia_vm_types/src/inference_core/dispatch_resolver/core_match.rs:1"
  "subset_julia_vm_types/src/inference_core/type_core.rs:0"
  "subset_julia_vm_types/src/inference_core/type_core/match.rs:0"
  "subset_julia_vm_types/src/inference_core/type_core/intersect.rs:0"
  "subset_julia_vm_types/src/inference_core/type_core/convert.rs:0"
  "subset_julia_vm_types/src/inference_core/type_core/registry.rs:0"
  "subset_julia_vm_types/src/inference_core/type_core/subtype.rs:0"
  "subset_julia_vm_types/src/inference_core/type_core/repr.rs:0"
  "subset_julia_vm_types/src/inference_core/specificity.rs:0"
  "subset_julia_vm_types/src/inference_core/selection.rs:0"
  "subset_julia_vm_types/src/inference_core/subtype.rs:0"
  "subset_julia_vm_types/src/inference_core/primitive_numeric.rs:0"
  "subset_julia_vm_vm/src/vm/dispatch.rs:1"
  "subset_julia_vm_compile/src/compile/expr/call/mod.rs:0"
  "subset_julia_vm_compile/src/compile/expr/call/dispatch.rs:0"
  # Includes four legacy `struct_table.values().find(type_id)` scans whose
  # duplicate-id ambiguity is tracked by Issue #11167; see the inventory.
  "subset_julia_vm_compile/src/compile/expr/call/constructors.rs:9"
  "subset_julia_vm_compile/src/compile/expr/call/module_call.rs:0"
  "subset_julia_vm_compile/src/compile/expr/call/dynamic.rs:0"
  "subset_julia_vm_compile/src/compile/expr/call/nary.rs:0"
  "subset_julia_vm_compile/src/compile/expr/call/handlers/math.rs:1"
)

count_file() {
  # Count hash-collection iteration patterns in a whitespace-squashed copy of
  # the file, so multi-line rustfmt chains (`c\n .method_tables\n .iter()`)
  # are still detected.
  local file="$1"
  local squashed mt kv overlap
  squashed=$(tr -d '[:space:]' < "$file")
  mt=$(printf '%s' "$squashed" | grep -o 'method_tables\.\(iter()\|iter_mut()\|keys()\|values()\|values_mut()\|into_iter()\|drain(\|retain(\)' | wc -l | tr -d ' ')
  kv=$(printf '%s' "$squashed" | grep -o '\.\(keys()\|values()\|values_mut()\)' | wc -l | tr -d ' ')
  # method_tables.keys()/values() is matched by both patterns; subtract the
  # overlap so it is counted once.
  overlap=$(printf '%s' "$squashed" | grep -o 'method_tables\.\(keys()\|values()\|values_mut()\)' | wc -l | tr -d ' ')
  echo $((mt + kv - overlap))
}

fail=0
for entry in "${BASELINES[@]}"; do
  file="${entry%:*}"
  baseline="${entry##*:}"
  if [ ! -f "$file" ]; then
    echo "ERROR: audited file missing (update the baseline list): $file"
    fail=1
    continue
  fi
  actual=$(count_file "$file")
  if [ "$actual" -gt "$baseline" ]; then
    echo "FAIL: $file has $actual hash-collection iteration site(s) (baseline $baseline)."
    echo "      New HashMap/HashSet iteration in the dispatch path — determinize it"
    echo "      (definition-order Vec / explicit sort) or prove order-independence,"
    echo "      classify it in memory/project/project_8641_dispatch_hashmap_iteration_inventory.md,"
    echo "      and bump the baseline in the same PR. See docs/vm/CODE_AUDITS.md (Issue #8659)."
    fail=1
  elif [ "$actual" -lt "$baseline" ]; then
    echo "NOTE: $file is below baseline ($actual < $baseline) — lower the baseline to tighten the ratchet."
  fi
done

if [ "$fail" -ne 0 ]; then
  exit 1
fi
echo "OK: dispatch-path hash-collection iteration counts are within baselines."
