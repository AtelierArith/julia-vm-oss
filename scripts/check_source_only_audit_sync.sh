#!/usr/bin/env bash
# check_source_only_audit_sync.sh — registry vs. premerge/CI drift gate
# (Issue #10870).
#
# `scripts/source_only_audits.tsv` is the canonical registry of fast,
# source-only audits that belong in the guarded local merge gate
# (`scripts/premerge_gate.sh`, via `scripts/run_source_only_audits.sh`) and,
# where `ci_enforced=true`, in `.github/workflows/ci.yml`. Before Issue
# #10870 there was no single place recording that intent, so an audit could
# be registered on one side (ci.yml) and quietly missing from the other
# (premerge_gate.sh) with nothing to notice — exactly how
# `check_structural_debt_inventory.sh` and `check_panic_free_ratchet.sh` drifted
# red on `main` while CI (disabled) still "covered" them on paper.
#
# This is a READ-ONLY drift check: it never edits `.github/workflows/*.yml`
# (the automation token lacks the `workflow` OAuth scope to push there — see
# docs/vm/CODE_AUDITS.md "When automation cannot update ci.yml"). It fails
# when:
#   1. a `premerge_default=true` registry row's script is not actually
#      invoked from `scripts/run_source_only_audits.sh`'s default gate path
#      (i.e. `scripts/premerge_gate.sh` no longer wires the runner in), or
#   2. either required Issue #11065 ownership row is absent, a registry boolean
#      is not exactly true/false, or a `ci_enforced=true` row has no executable
#      `run: bash scripts/<script>` step in `.github/workflows/ci.yml`, or
#   3. a `ci_enforced=false` row (a known, tracked CI-registration gap) has
#      no `issue` reference to explain and track the gap — the explicit
#      allowlist the Issue #10870 acceptance criteria calls for.
#
# Usage:
#   bash scripts/check_source_only_audit_sync.sh
#
# Exit code: 0 = registry, premerge_gate.sh, and ci.yml agree (or every gap
#            is explicitly issue-tracked); 1 otherwise.

set -uo pipefail

cd "$(dirname "$0")/.." || exit 1

REGISTRY="scripts/source_only_audits.tsv"
RUNNER="scripts/run_source_only_audits.sh"
PREMERGE="scripts/premerge_gate.sh"
CI_WORKFLOW=".github/workflows/ci.yml"

for f in "$REGISTRY" "$RUNNER" "$PREMERGE" "$CI_WORKFLOW"; do
  if [ ! -f "$f" ]; then
    echo "ERROR: expected file missing: $f" >&2
    exit 2
  fi
done

failures=0
checked=0
first_line=1

require_default_row() {
  required_name="$1"
  required_script="$2"
  required_issue="${3:-#11065}"
  if ! awk -F '\t' -v name="$required_name" -v script="$required_script" '
      $1 == name && $2 == script && $3 == "true" { found = 1 }
      END { exit(found ? 0 : 1) }
    ' "$REGISTRY"; then
    echo "ERROR: missing required default registry row '$required_name' for scripts/$required_script (Issue $required_issue)" >&2
    failures=$((failures + 1))
  fi
}

# scripts/premerge_gate.sh must actually EMIT the registry-driven runner as
# part of its default command list. Grepping source text is insufficient: the
# executable line can be removed while comments still mention the basename
# (Issue #11065).
gate_list="$(bash "$PREMERGE" --list-gates 2>&1)"
gate_list_code=$?
if [ "$gate_list_code" -ne 0 ]; then
  echo "ERROR: $PREMERGE --list-gates failed (exit $gate_list_code):" >&2
  printf '%s\n' "$gate_list" | sed 's/^/  /' >&2
  failures=$((failures + 1))
elif ! printf '%s\n' "$gate_list" | grep -Fqx '  bash scripts/run_source_only_audits.sh'; then
  echo "ERROR: $PREMERGE default gate list does not include the exact source-only runner command" >&2
  echo "  '  bash scripts/run_source_only_audits.sh' — the registry (Issue #10870/#11065)" >&2
  echo "  is not wired into the executable guarded premerge path." >&2
  failures=$((failures + 1))
fi

# Audit self-test target ownership is part of the default guarded gate. Require
# the executable command, including its changed-path base, so comments or a
# full-suite-only authoring workflow cannot masquerade as merge coverage.
if ! printf '%s\n' "$gate_list" | grep -Fqx \
    '  bash scripts/check_audit_negative_selftest.sh --changed-from origin/main'; then
  echo "ERROR: $PREMERGE default gate list does not include the exact changed-target audit self-test command" >&2
  echo "  '  bash scripts/check_audit_negative_selftest.sh --changed-from origin/main' (Issue #11274)." >&2
  failures=$((failures + 1))
fi

# Semantic-pipeline changes must add the bounded metamorphic matrix
# automatically, while docs-only changes must not pay that build/run cost.
# The environment variable is a test-only path-list hook consumed by
# premerge_gate.sh's pure --list-gates mode (Issue #10452 closure audit).
for semantic_path in \
  subset_julia_vm_lowering/src/lowering/example.rs \
  subset_julia_vm_compile/src/compile/example.rs \
  subset_julia_vm_vm/src/vm/example.rs \
  subset_julia_vm/src/julia/base/operators.jl \
  subset_julia_vm/src/runtime_types/example.rs \
  subset_julia_vm_parser/src/parser.rs; do
  semantic_gate_list="$(SJULIA_PREMERGE_CHANGED_PATHS="$semantic_path" \
    bash "$PREMERGE" --list-gates 2>&1)"
  if ! printf '%s\n' "$semantic_gate_list" | grep -Fqx '  bash scripts/metamorphic_equivalence.sh'; then
    echo "ERROR: semantic path $semantic_path does not automatically select the metamorphic premerge gate" >&2
    failures=$((failures + 1))
  fi
done
docs_gate_list="$(SJULIA_PREMERGE_CHANGED_PATHS='docs/vm/example.md' \
  bash "$PREMERGE" --list-gates 2>&1)"
if printf '%s\n' "$docs_gate_list" | grep -Fqx '  bash scripts/metamorphic_equivalence.sh'; then
  echo "ERROR: docs-only changes unexpectedly select the metamorphic premerge gate" >&2
  failures=$((failures + 1))
fi

# These are the concrete source-only ownership gaps fixed by Issue #11065.
# The registry is canonical for execution, while this bounded ratchet ensures
# deleting the ownership declaration itself cannot silently shrink the gate.
require_default_row "compile_vm_coupling" "audit_compile_vm_coupling.sh"
require_default_row "fixture_categories" "check_fixture_categories.sh"
require_default_row "fixture_coverage_contract_selftest" "fixture_coverage_contract_selftest.sh"
require_default_row "definition_order_merges" "check_definition_order_merges.sh"
require_default_row "status_done_archive_budget" "check_status_done_archive_budget.sh"
require_default_row "constructor_owner_resolution" "check_constructor_owner_resolution.sh"
require_default_row "binding_provenance_authority" "check_binding_provenance_authority.sh"
require_default_row "base_exports_subset" "check_base_exports_subset.sh"
require_default_row "source_position_chronology" "check_source_position_chronology.sh"
require_default_row "builtin_type_registry" "check_builtin_type_registry.sh"
require_default_row "constructor_return_identity" "check_constructor_return_identity.sh"
require_default_row "base_cache_schema_fingerprint" "audit_base_cache_schema_fingerprint.sh" "#10688"
require_default_row "exception_payload_carrier" "audit_exception_payload_carrier.sh" "#11647"

# shellcheck disable=SC2034  # notes is read for the full TSV row shape; not used by this check
while IFS=$'\t' read -r name script premerge_default ci_enforced issue notes; do
  if [ "$first_line" -eq 1 ]; then
    first_line=0
    continue
  fi
  [ -z "$name" ] && continue
  case "$name" in \#*) continue ;; esac
  checked=$((checked + 1))

  case "$premerge_default" in
    true|false) ;;
    *)
      echo "ERROR: registry row '$name' has invalid premerge_default='$premerge_default' (expected true or false)" >&2
      failures=$((failures + 1))
      ;;
  esac
  case "$ci_enforced" in
    true|false) ;;
    *)
      echo "ERROR: registry row '$name' has invalid ci_enforced='$ci_enforced' (expected true or false)" >&2
      failures=$((failures + 1))
      ;;
  esac

  if [ ! -f "scripts/$script" ]; then
    echo "ERROR: registry row '$name' points at scripts/$script, which does not exist" >&2
    failures=$((failures + 1))
    continue
  fi

  if [ "$ci_enforced" = "true" ]; then
    if ! awk -v command="bash scripts/$script" '
        /^[[:space:]]*run:[[:space:]]*/ {
          line = $0
          sub(/^[[:space:]]*run:[[:space:]]*/, "", line)
          if (line == command) found = 1
        }
        END { exit(found ? 0 : 1) }
      ' "$CI_WORKFLOW"; then
      echo "ERROR: registry row '$name' claims ci_enforced=true for scripts/$script," >&2
      echo "  but $CI_WORKFLOW has no executable 'run: bash scripts/$script' step." >&2
      echo "  A shellcheck/comment-only mention does not execute the audit. Add the step" >&2
      echo "  or flip ci_enforced to false with an issue reference (Issue #10870)." >&2
      failures=$((failures + 1))
    fi
  else
    if [ -z "$issue" ] || [ "$issue" = "-" ]; then
      echo "ERROR: registry row '$name' has ci_enforced=false but no 'issue' column" >&2
      echo "  tracking the pending CI-registration gap — this is the explicit" >&2
      echo "  allowlist Issue #10870 requires; add a tracking issue reference." >&2
      failures=$((failures + 1))
    fi
  fi
done < "$REGISTRY"

if [ "$failures" -gt 0 ]; then
  echo "FAIL: $failures source-only audit registry/CI/premerge drift issue(s) (Issue #10870)" >&2
  exit 1
fi

echo "OK: source-only audit registry ($REGISTRY) is in sync with $PREMERGE and $CI_WORKFLOW ($checked rows, Issue #10870)."
