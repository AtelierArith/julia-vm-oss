#!/usr/bin/env bash
# Reproduce the static evidence table for the backend-strategy ADR
# (Issue #8639, measured for #8651; decision record:
# docs/vm/ADR_BACKEND_STRATEGY.md).
#
# Prints the machine-derivable inputs the ADR is based on: AoT footprint
# (LOC, files, feature-gate boundary), crate coupling, dependency surface,
# and git activity. Read-only: no builds, no network. Run from anywhere
# inside the repo.
#
# Build-time numbers (cargo check deltas, scripts/test_aot.sh wall time) are
# load-dependent and are NOT reproduced here; the measurement protocol and
# the 2026-07-02 numbers are recorded in the ADR itself. To re-measure:
#   1. rm -rf target && time cargo check -p subset_julia_vm
#   2. touch subset_julia_vm/src/lib.rs && time cargo check -p subset_julia_vm
#   3. time cargo check -p subset_julia_vm --features aot
#   4. time cargo check -p subset_julia_vm --features cranelift
#   5. time bash scripts/test_aot.sh
# Compare (3) and (4) against (2), back-to-back on the same machine.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

loc() { find "$@" -name '*.rs' -print0 | xargs -0 wc -l | tail -1 | awk '{print $1}'; }

section() { printf '\n== %s ==\n' "$1"; }

section "AoT footprint (lines of Rust)"
aot_loc=$(loc subset_julia_vm/src/aot)
runtime_loc=$(loc subset_julia_vm_runtime/src)
vm_loc=$(loc subset_julia_vm/src)
aot_files=$(find subset_julia_vm/src/aot -name '*.rs' | wc -l)
printf 'subset_julia_vm/src/aot/      %8s LOC in %s files\n' "$aot_loc" "$aot_files"
for d in analyze codegen inference ir optimizer; do
  printf '  aot/%-10s %8s LOC\n' "$d" "$(loc "subset_julia_vm/src/aot/$d")"
done
printf 'subset_julia_vm_runtime/src/  %8s LOC\n' "$runtime_loc"
printf 'subset_julia_vm/src/ total    %8s LOC (aot/ = %s%%)\n' \
  "$vm_loc" "$(awk "BEGIN{printf \"%.1f\", 100*$aot_loc/$vm_loc}")"
printf 'AoT-gated test files:\n'
wc -l subset_julia_vm/tests/aot_e2e_tests.rs subset_julia_vm/tests/core_ir_aot_tests.rs

section "Feature-gate boundary (cfg(feature = \"aot\") outside src/aot/)"
grep -rln 'feature = "aot"' subset_julia_vm/src --include='*.rs' | grep -v '/aot/' | sort

section "Module coupling: what aot/ imports from the VM crate"
grep -rhoE 'crate::[a-z_]+' subset_julia_vm/src/aot --include='*.rs' \
  | grep -v 'crate::aot' | sort | uniq -c | sort -rn

section "Reverse coupling: non-aot code referencing aot:: (excl. bins)"
grep -rn '\baot::' subset_julia_vm/src --include='*.rs' \
  | grep -v 'src/aot/' | grep -v 'src/bin/' | grep -v '^\s*//' || echo '(none)'

section "Shipping-target usage of AoT (FFI / WASM crates)"
if grep -rn '\baot\b' subset_julia_vm_ffi/src subset_julia_vm_web/src 2>/dev/null; then
  echo 'AoT IS referenced by shipping targets'
else
  echo 'no references — iOS (FFI) and WASM ship the bytecode VM only'
fi

section "Optional dependency surface"
echo 'feature aot:'
grep -E '^aot =' subset_julia_vm/Cargo.toml
echo 'feature cranelift:'
grep -E '^cranelift =' subset_julia_vm/Cargo.toml

section "Git activity: commits touching aot paths, by month (last 9 months)"
since="$(date -d '9 months ago' +%Y-%m-01 2>/dev/null || date -v-9m +%Y-%m-01)"
paste <(git log --since="$since" --date=format:%Y-%m --format=%ad \
          -- subset_julia_vm/src/aot subset_julia_vm_runtime | sort | uniq -c) \
      /dev/null | awk '{printf "  %s: %s aot-path commits\n", $2, $1}'
total=$(git log --since="$since" --oneline | wc -l)
aot_total=$(git log --since="$since" --oneline \
  -- subset_julia_vm/src/aot subset_julia_vm_runtime | wc -l)
printf 'total commits since %s: %s (aot-path: %s = %s%%)\n' \
  "$since" "$total" "$aot_total" \
  "$(awk "BEGIN{printf \"%.1f\", 100*$aot_total/$total}")"
