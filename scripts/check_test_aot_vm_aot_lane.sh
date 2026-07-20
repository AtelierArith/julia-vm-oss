#!/usr/bin/env bash
# check_test_aot_vm_aot_lane.sh — Issue #10815
#
# This repo has had THREE "an audit/gate exists on paper but nothing local
# actually runs it" incidents (#10870, #10912, and the `check_name_based_lookup.sh`
# CI-only registration gap closed by #11078). Issue #10815 found the same
# failure shape one layer up: `scripts/metamorphic_equivalence.sh`'s `vm_aot`
# differential lane (VM vs AoT stdout over a curated corpus,
# `tests/equivalence/vm_aot.tsv`) existed and was correct, but the ONLY thing
# that ran it was `premerge_gate.sh --metamorphic` (auto-selected for
# `subset_julia_vm/src/*` changes) at LEAD certification time — the mandatory
# per-change AoT gate every implementation agent actually runs locally
# (`bash scripts/test_aot.sh`, AGENTS.md hard rule #8) never touched it. Five
# VM/AoT semantic-drift bugs (#10796/#10731/#10663/#10537/#10523) landed in one
# week undetected by that gate; three more (#11180/#11181/#11182) surfaced the
# moment the corpus was actually widened and exercised.
#
# This is a SOURCE-ONLY audit (no built binaries needed) that pins two things
# so this specific gap cannot silently reopen:
#
#   1. `scripts/test_aot.sh` must still invoke BOTH
#      `metamorphic_equivalence.sh ... --lane vm_aot` (the differential lane
#      itself) AND `metamorphic_equivalence.sh ... --selftest` (its own
#      negative self-test, proving the comparators still fire) as part of the
#      gate every AoT-touching change runs.
#   2. `tests/equivalence/vm_aot.tsv` must have at least
#      SJULIA_VM_AOT_MIN_ROWS data rows (default 11) — a ratchet against the
#      corpus silently shrinking back toward the original 3-acceptance-kernel
#      scope. Growing the corpus never requires bumping this floor; shrinking
#      it does, and that edit is the review signal.
#   3. Every AoT binary consumer must derive its default `sjulia` / `juliars`
#      path from Cargo's effective target directory while preserving explicit
#      overrides. The executable contract test covers metadata/config, default,
#      external, relative, and overridden paths (Issues #11598/#11695).
#   4. The nightly upstream-fixture parity command must include the `scope`
#      category that exposed #11599 (Issue #11693).
#
# Usage (from the repository root):
#   bash scripts/check_test_aot_vm_aot_lane.sh
#
# Overrides for testing:
#   SJULIA_TEST_AOT_SCRIPT=<file> SJULIA_VM_AOT_TSV=<file> \
#   SJULIA_VM_AOT_MIN_ROWS=<n> bash scripts/check_test_aot_vm_aot_lane.sh
#
# Exit code: 0 = both invocations present and the corpus meets the floor;
#            1 = either invocation is missing or the corpus shrank below the
#            floor; 2 = infrastructure failure (expected file missing).

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

TEST_AOT="${SJULIA_TEST_AOT_SCRIPT:-$REPO_ROOT/scripts/test_aot.sh}"
VM_AOT_TSV="${SJULIA_VM_AOT_TSV:-$REPO_ROOT/tests/equivalence/vm_aot.tsv}"
MIN_ROWS="${SJULIA_VM_AOT_MIN_ROWS:-11}"

if [ ! -f "$TEST_AOT" ]; then
  echo "FAIL: scripts/test_aot.sh not found at $TEST_AOT (Issue #10815)" >&2
  exit 2
fi
if [ ! -f "$VM_AOT_TSV" ]; then
  echo "FAIL: vm_aot corpus not found at $VM_AOT_TSV (Issue #10815)" >&2
  exit 2
fi

status=0

if ! grep -Fq 'metamorphic_equivalence.sh" --lane vm_aot' "$TEST_AOT"; then
  echo "FAIL: scripts/test_aot.sh no longer invokes 'metamorphic_equivalence.sh ... --lane vm_aot' (Issue #10815)." >&2
  echo "  The mandatory AoT gate (AGENTS.md hard rule #8, 'bash scripts/test_aot.sh')" >&2
  echo "  would stop exercising the VM-vs-AoT differential lane on every AoT-touching" >&2
  echo "  change, reopening the gap #10815 documented (an audit that exists but" >&2
  echo "  nothing local actually runs — same failure class as #10870/#10912)." >&2
  status=1
fi

if ! grep -Fq 'metamorphic_equivalence.sh" --selftest' "$TEST_AOT"; then
  echo "FAIL: scripts/test_aot.sh no longer invokes 'metamorphic_equivalence.sh ... --selftest' (Issue #10815)." >&2
  echo "  Without this, nothing proves the vm_aot lane's comparators (value/type/" >&2
  echo "  exception) still fire on a seeded divergence — a silently-broken lane" >&2
  echo "  would keep reporting green (the #9129 F2 failure mode)." >&2
  status=1
fi

row_count="$(awk -F'\t' '
  /^[[:space:]]*#/ { next }
  NF < 2 { next }
  $1 == "name" { next }
  { count++ }
  END { print count + 0 }
' "$VM_AOT_TSV")"

if [ "$row_count" -lt "$MIN_ROWS" ]; then
  echo "FAIL: tests/equivalence/vm_aot.tsv has $row_count case(s), below the Issue #10815 floor of $MIN_ROWS." >&2
  echo "  The vm_aot differential corpus shrank back toward acceptance-kernel-only" >&2
  echo "  coverage; #10815's evidence (5 VM/AoT semantic-drift bugs in one week) is" >&2
  echo "  exactly the class this floor exists to keep detectable. If a case was" >&2
  echo "  removed for a legitimate reason, lower SJULIA_VM_AOT_MIN_ROWS's default in" >&2
  echo "  this script in the same PR and say why; do not just let the count drop." >&2
  status=1
fi

if ! (cd "$REPO_ROOT" && python3 -m unittest tests/test_aot_binary_path_contract.py); then
  echo "FAIL: AoT harness/nightly contract regression tests failed (Issues #11598/#11693)." >&2
  status=1
fi

if [ "$status" -eq 0 ]; then
  echo "OK: scripts/test_aot.sh wires the vm_aot differential lane + its selftest, tests/equivalence/vm_aot.tsv has $row_count case(s) (>= $MIN_ROWS floor, Issue #10815), AoT binary consumers honor Cargo target paths (Issue #11598), and nightly fixture parity covers scope (Issue #11693)."
fi

exit "$status"
