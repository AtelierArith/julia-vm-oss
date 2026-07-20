#!/usr/bin/env bash
# run_source_only_audits.sh — run the registered default source-only audit
# gate set (Issue #10870).
#
# `scripts/premerge_gate.sh` — the guarded final-verification sequence for
# lead merges (Issue #9644) — is the ONLY thing standing between a red
# ratchet audit and `main` while GitHub Actions is disabled. Before Issue
# #10870, the default premerge gate list only ran
# `check_no_new_domain_builtins.sh` and `audit_base_cache_schema_fingerprint.sh`;
# `check_structural_debt_inventory.sh` and `check_panic_free_ratchet.sh` were
# registered in ci.yml but never wired into premerge, so both drifted red on
# `main` (again — same failure class as #8740 / #9920-#9925) without any
# guarded merge ever catching it.
#
# This script is the single runner both `premerge_gate.sh` and CI consume: it
# reads `scripts/source_only_audits.tsv` (the canonical registry) and runs
# every row with `premerge_default=true`, so the registry — not a hand-copied
# list embedded in premerge_gate.sh — is what decides which audits are part
# of the guarded merge gate. Adding a new fast source-only audit to the
# default gate means adding ONE registry row, not editing premerge_gate.sh.
#
# On failure, prints which registered audit(s) failed by NAME so the report
# can never be silently swallowed (Issue #9129 failure mode F5).
#
# Usage:
#   bash scripts/run_source_only_audits.sh
#
# Exit code: 0 if every premerge_default=true audit in the registry exits 0;
#            1 if any fails (each failing audit's name + full output is
#            printed, followed by a summary line naming every failed audit).

set -uo pipefail

cd "$(dirname "$0")/.." || exit 1

REGISTRY="${SJULIA_SOURCE_ONLY_AUDIT_REGISTRY:-scripts/source_only_audits.tsv}"

if [ ! -f "$REGISTRY" ]; then
  echo "ERROR: source-only audit registry not found: $REGISTRY" >&2
  exit 2
fi

total=0
failed_names=""
first_line=1

# Registration completeness is metadata about the audit set itself, not one
# source invariant row. Run its cheap mode before the registry so a newly added
# unregistered checker cannot reach main merely because the full injected suite
# is intentionally not part of every premerge (Issue #11065).
echo "[run_source_only_audits] running: audit_registration_completeness (scripts/check_audit_negative_selftest.sh --registration-only)"
if ! registration_out="$(bash scripts/check_audit_negative_selftest.sh --registration-only 2>&1)"; then
  echo "$registration_out"
  echo "[run_source_only_audits] FAIL: source-only meta-audit 'audit_registration_completeness' failed (Issue #11065)"
  failed_names="$failed_names audit_registration_completeness"
else
  echo "$registration_out"
fi

# shellcheck disable=SC2034  # ci_enforced/notes are read for the full TSV row shape; not used by this runner
while IFS=$'\t' read -r name script premerge_default ci_enforced issue notes; do
  if [ "$first_line" -eq 1 ]; then
    first_line=0
    continue
  fi
  [ -z "$name" ] && continue
  case "$name" in \#*) continue ;; esac
  [ "$premerge_default" = "true" ] || continue

  total=$((total + 1))
  echo "[run_source_only_audits] running: $name (scripts/$script)"
  if ! out="$(bash "scripts/$script" 2>&1)"; then
    echo "$out"
    echo "[run_source_only_audits] FAIL: source-only audit '$name' (scripts/$script) failed (Issue $issue)"
    failed_names="$failed_names $name"
  fi
done < "$REGISTRY"

if [ -n "$failed_names" ]; then
  echo "[run_source_only_audits] FAIL:$failed_names"
  exit 1
fi

echo "[run_source_only_audits] OK: $total/$total registered source-only audits green (registry: $REGISTRY)"
