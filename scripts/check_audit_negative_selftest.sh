#!/usr/bin/env bash
# shellcheck disable=SC2329  # inject_* and cleanup are invoked indirectly (via "$inject_fn" / trap)
# check_audit_negative_selftest.sh — audit-the-audits negative self-tests
# (Issue #9129, extended for the remaining audits in Issues #9388 and #9463).
#
# Failure mode F2 from Issue #9129: the #8655/#8656 crate split silently broke
# three audit scripts. They kept exiting 0 while no longer reading the sources
# they were meant to guard. A defence that reports "OK" after it has stopped
# looking is worse than no defence, because the changes it should catch now go
# through unguarded.
#
# This framework proves each COVERED audit still FAILS on a known-bad input:
#   1. copy the working-tree sources into a throwaway sandbox,
#   2. inject a genuine violation of the invariant that audit guards,
#   3. run the audit against the sandbox,
#   4. require a NON-ZERO exit AND a matching human-readable reason on output.
#
# Injection-specific reasons (Issue #9388): the negative control does NOT assume
# the audit passes on a clean tree. Several ratchet/allowlist audits are red on
# main today (baseline drift, Issue #8740), so requiring a clean exit 0 would
# make this framework un-runnable. Instead, each self-test picks a reason string
# that the audit emits ONLY in response to the injection: the harness runs the
# audit on the clean sandbox first, REQUIRES the reason to be ABSENT there, then
# injects and REQUIRES the reason to be PRESENT and the exit non-zero. That
# proves the audit's guard code fired because of the injected violation, whether
# or not the audit was already failing for unrelated (drifted-baseline) reasons.
# When the clean sandbox exits 0 the harness also reports the positive control.
#
# Coverage bookkeeping: every scripts/check_*.sh + scripts/audit_*.sh must be
# either registered with a `run_selftest` here OR listed in NO_SELFTEST_REASONS
# below with a written reason. The fast `--registration-only` mode executes
# these same top-level registration calls without sandboxes and fails if any
# audit is unaccounted for, so a newly added audit cannot silently skip either
# the full suite or guarded premerge.
#
# It also lints every scripts/check_*.sh + scripts/audit_*.sh for a
# failure-diagnostic emit — a silent `exit 1` hides which audit broke
# (Issue #9129 failure mode F5 / the `set -e`-swallowed FAIL report in PR #9095).
#
# Usage (from the repository root):
#   bash scripts/check_audit_negative_selftest.sh
#   bash scripts/check_audit_negative_selftest.sh --registration-only
#   bash scripts/check_audit_negative_selftest.sh --target-path <repo-relative-path>
#   bash scripts/check_audit_negative_selftest.sh --changed-from <git-ref>
#   bash scripts/check_audit_negative_selftest.sh --list-targets
#
# Exit code: 0 = every covered audit detects its injected violation with a
#            stated, injection-specific reason, every audit is covered or
#            annotated, and no audit script can fail silently; 1 otherwise.
#
# Dependencies: python3 (stdlib only, for the enum injections), bash 3.2+.

# NOTE: intentionally NO `set -e` — this script runs audits that are EXPECTED to
# exit non-zero, and inspects their exit codes by hand.
set -uo pipefail

REGISTRATION_ONLY=0
LIST_TARGETS=0
FILTER_ACTIVE=0
EXPLICIT_TARGET=0
TARGET_PATHS=""
CHANGED_FROM=""
while [ "$#" -gt 0 ]; do
  case "$1" in
    --registration-only)
      REGISTRATION_ONLY=1
      shift
      ;;
    --list-targets)
      LIST_TARGETS=1
      shift
      ;;
    --target-path)
      [ "$#" -ge 2 ] || { echo "FAIL: --target-path requires a path" >&2; exit 2; }
      FILTER_ACTIVE=1
      EXPLICIT_TARGET=1
      TARGET_PATHS="$TARGET_PATHS
$2"
      shift 2
      ;;
    --changed-from)
      [ "$#" -ge 2 ] || { echo "FAIL: --changed-from requires a git ref" >&2; exit 2; }
      FILTER_ACTIVE=1
      [ -z "$CHANGED_FROM" ] || { echo "FAIL: --changed-from may be specified only once" >&2; exit 2; }
      CHANGED_FROM="$2"
      shift 2
      ;;
    *)
      echo "FAIL: unknown argument: $1" >&2
      exit 2
      ;;
  esac
done

if [ "$REGISTRATION_ONLY" -eq 1 ] && { [ "$LIST_TARGETS" -eq 1 ] || [ "$FILTER_ACTIVE" -eq 1 ]; }; then
  echo "FAIL: --registration-only cannot be combined with target-selection modes" >&2
  exit 2
fi
if [ "$LIST_TARGETS" -eq 1 ] && [ "$FILTER_ACTIVE" -eq 1 ]; then
  echo "FAIL: --list-targets cannot be combined with target-selection modes" >&2
  exit 2
fi

cd "$(dirname "$0")/.." || exit 1
REPO_ROOT="$(pwd)"
if [ -n "$CHANGED_FROM" ]; then
  if ! changed_paths="$(git diff --name-only "$CHANGED_FROM"...HEAD 2>&1)"; then
    echo "FAIL: cannot compute changed audit targets from '$CHANGED_FROM':" >&2
    printf '%s\n' "$changed_paths" | sed 's/^/  /' >&2
    exit 2
  fi
  TARGET_PATHS="$TARGET_PATHS
$changed_paths"
fi

# Source paths copied into the sandbox (relative to the repo root). The whole
# crate `src/` trees plus the ratchet baseline TSVs are copied so a covered
# audit's negative control runs exactly as it does in CI. Extend only when a new
# audit reads sources/data outside this set. `target/`, `.git/`, the large
# `julia/` submodule, and the bulk fixture tree are deliberately excluded; only
# small, explicitly needed fixture manifests/corpora are copied.
SANDBOX_PATHS=(
  Cargo.toml
  rust-toolchain.toml
  build.sh
  scripts
  .github/workflows/ci.yml
  .github/workflows/nightly-gates.yml
  .github/workflows/pr-fast.yml
  .github/aot-gate-paths.txt
  subset_julia_vm/Cargo.toml
  subset_julia_vm/src
  subset_julia_vm/build.rs
  julia/base/exports.jl
  subset_julia_vm_lowering/Cargo.toml
  subset_julia_vm_lowering/src
  subset_julia_vm_compile/Cargo.toml
  subset_julia_vm_compile/build.rs
  subset_julia_vm_compile/src
  subset_julia_vm_vm/Cargo.toml
  subset_julia_vm_vm/src
  subset_julia_vm/tests/regression_dispatch_inference_tests.rs
  tests/test_aot_gate_selection.py
  tests/test_aot_binary_path_contract.py
  subset_julia_vm/tests/fixtures/regex/regex_split_10176.jl
  subset_julia_vm/tests/fixtures/dispatch_parity/corpus.toml
  subset_julia_vm/tests/fixtures/struct/manifest.toml
  subset_julia_vm/tests/fixtures/struct/global_new_helper_11005.jl
  subset_julia_vm/tests/fixtures/struct/ownerless_new_lookup_11204.jl
  subset_julia_vm/tests/fixtures/struct/ownerless_new_keyword_lookup_11204.jl
  subset_julia_vm/tests/fixtures/struct/ownerless_parametric_new_lookup_11204.jl
  subset_julia_vm/tests/fixtures/dispatch/manifest.toml
  subset_julia_vm/tests/fixtures/dispatch/constructor_return_exact_or_any_11436.jl
  subset_julia_vm_types/src
  subset_julia_vm_types/Cargo.toml
  subset_julia_vm_bytecode/src
  subset_julia_vm_bytecode/Cargo.toml
  subset_julia_vm_ffi/src
  subset_julia_vm_ffi/Cargo.toml
  subset_julia_vm_parser/src
  subset_julia_vm_parser/Cargo.toml
  subset_julia_vm_parser_common/Cargo.toml
  subset_julia_vm_runtime/src
  subset_julia_vm_runtime/Cargo.toml
  subset_julia_vm_web/src
  subset_julia_vm_web/Cargo.toml
  subset_julia_vm_ir/Cargo.toml
  docs/vm/RUST_TOOLCHAIN.md
  docs/vm/AUDIT_SELFTEST_ANCHORS.tsv
  docs/vm/rust_semantics_classification.tsv
  docs/vm/NUMERIC_MATRIX_FULL_ALLOWLIST.tsv
  docs/vm/GENERATOR_TRAIT_MATRIX_SKIPLIST.tsv
  docs/vm/ERROR_SPAN_RATCHET_BASELINE.tsv
  docs/vm/PANIC_FREE_RATCHET_BASELINE.tsv
  docs/vm/PANIC_FREE_DENY_MODULES.tsv
  docs/vm/PANIC_FREE_PRODUCTION_BASELINE.tsv
  docs/vm/DEFINITION_ORDER_MERGE_INVENTORY.tsv
  docs/vm/CONSTRUCTOR_OWNER_FALLBACK_INVENTORY.tsv
  docs/vm/CONSTRUCTOR_RETURN_IDENTITY_INVENTORY.tsv
  docs/vm/BINDING_PROVENANCE_CONSUMERS.tsv
  docs/vm/EXCEPTION_TAXONOMY_JULIA_BASELINE.tsv
  docs/vm/UNSAFE_INVENTORY_BASELINE.tsv
  docs/vm/WORKAROUNDS.md
  docs/vm/STATUS.md
  docs/vm/DONE.md
  docs/vm/BINARY_DISPATCH.md
  docs/vm/COLLECTIONS.md
  docs/vm/TEST_BINARY_ALLOWLIST.tsv
  docs/vm/FIXTURE_CATEGORIES.tsv
  docs/vm/BASE_DUPLICATE_SIGNATURE_ALLOWLIST.tsv
  subset_julia_vm/tests/fixtures/generator/generator_trait_matrix_9566.tsv
  subset_julia_vm/tests/fixtures/generator/generator_trait_matrix_9566.jl
  subset_julia_vm/tests/fixtures/types/type_application_matrix_10556.jl
  docs/vm/TYPE_APPLICATION_MATRIX_SKIPLIST.tsv
  tests/equivalence/vm_aot.tsv
)

overall_fail=0
COVERED_LIST=" "   # space-delimited basenames registered via run_selftest
TARGET_ROWS=""
SELECTED_COUNT=0

log()  { printf '%s\n' "$*"; }
pass() { printf 'PASS: %s\n' "$*"; }
bad()  { printf 'FAIL: %s\n' "$*"; overall_fail=1; }

# Copy SANDBOX_PATHS into $1 (a fresh dir), preserving relative structure.
populate_sandbox() {
  local dst="$1" p
  for p in "${SANDBOX_PATHS[@]}"; do
    if [ -e "$REPO_ROOT/$p" ]; then
      mkdir -p "$dst/$(dirname "$p")"
      cp -R "$REPO_ROOT/$p" "$dst/$p"
    fi
  done
  # inventory_rust_semantics.sh is invoked as ./scripts/... by its ratchet, so
  # the executable bit must survive the copy.
  chmod +x "$dst"/scripts/*.sh 2>/dev/null || true
}

# One pristine template sandbox is built once and only ever READ (baseline
# runs). Each negative control gets its own throwaway copy so injections stay
# isolated and the template never drifts between tests.
TEMPLATE_SANDBOX=""
CURRENT_TEST_SANDBOX=""
cleanup() { rm -rf "$TEMPLATE_SANDBOX" "$CURRENT_TEST_SANDBOX" 2>/dev/null || true; }
trap cleanup EXIT
if [ "$REGISTRATION_ONLY" -eq 0 ] && [ "$LIST_TARGETS" -eq 0 ]; then
  TEMPLATE_SANDBOX="$(mktemp -d)"
  populate_sandbox "$TEMPLATE_SANDBOX"
fi

# Run an inline Python injector against the sandbox copy of the shared
# fail-loud edit helper. This avoids importing the working tree while testing a
# staged mutation and keeps Python 3.9 compatibility under one implementation.
injector_python() {
  local sandbox="$1" target="$2"
  PYTHONPATH="$sandbox/scripts" python3 - "$sandbox/$target"
}

# --------------------------------------------------------------------------
# Injections — each mutates/creates one (occasionally two) staged source(s) in
# the sandbox ($1) to introduce a genuine violation of the audited invariant.
# Each embeds a unique marker so the harness can confirm the injection landed
# before trusting the audit's exit. All markers/reasons carry a 9129/9388 tag so
# they cannot collide with real source.
# --------------------------------------------------------------------------

# check_instr_wire_ids.sh COVERAGE violation: add an `Intrinsic` enum variant
# with no entry in `intrinsic_to_wire_id()`.
inject_wire_ids() {
  python3 - "$1/subset_julia_vm_bytecode/src/intrinsics.rs" <<'PY'
import re, sys
p = sys.argv[1]
s = open(p).read()
s, n = re.subn(r'(pub enum Intrinsic \{\n)',
               r'\1    SelftestBogusIntrinsic9129,\n', s, count=1)
assert n == 1, "could not find `pub enum Intrinsic {`"
open(p, 'w').write(s)
PY
}

# check_type_application_matrix.sh COVERAGE violation: add a new type-application
# opcode variant to the `Instr` enum with no `opcode-covered:` declaration in the
# parity matrix fixture (Issue #10556).
inject_type_application_matrix() {
  python3 - "$1/subset_julia_vm_bytecode/src/instr.rs" <<'PY'
import re, sys
p = sys.argv[1]
s = open(p).read()
s, n = re.subn(r'(pub enum Instr \{\n)',
               r'\1    ApplyTypeSelftestBogus10556(usize),\n', s, count=1)
assert n == 1, "could not find `pub enum Instr {`"
open(p, 'w').write(s)
PY
}

# check_dispatch_determinism.sh ratchet violation: introduce a new
# hash-collection iteration site (`.keys()`) in a baseline-0 dispatch-path file.
inject_dispatch() {
  printf '%s\n' '// audit-selftest SelftestInjectedHashIter9129 dummy.keys()' \
    >> "$1/subset_julia_vm_compile/src/compile/expr/call/mod.rs"
}

# check_dispatch_negative_oracle.sh violation: remove a required MethodError
# negative cell by renaming its case in the dispatch parity corpus.
inject_dispatch_negative_oracle() {
  python3 - "$1/subset_julia_vm/tests/fixtures/dispatch_parity/corpus.toml" <<'PY'
import sys
p = sys.argv[1]
s = open(p).read()
s = s.replace(
    'name = "neg_vector_invariance_methoderror_9567"',
    'name = "neg_vector_invariance_removed_9567"',
    1,
)
open(p, 'w').write(s)
PY
}

# check_no_typevar_name_heuristic.sh violation: reintroduce the old
# spelling-based TypeVar classifier.
inject_typevar_name_heuristic() {
  cat >> "$1/subset_julia_vm_types/src/types/julia_type/parsing.rs" <<'RS'

// audit-selftest SelftestTypeVarNameHeuristic9563
fn is_type_variable_name_selftest9563(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(c) if c.is_ascii_uppercase() => chars.all(|c| c.is_ascii_digit()),
        _ => false,
    }
}
RS
}

# check_name_based_lookup.sh violation: add a new TypeVar scope map keyed only
# by the display name string, the root #10279 collision class.
inject_name_based_lookup() {
  {
    printf '%s\n' ''
    printf '%s\n' '// audit-selftest SelftestNameBasedLookup10279'
    # The current clean count can be below the ratchet baseline after debt is
    # retired without the baseline being tightened immediately. Inject enough
    # same-shape sites to exceed the baseline even from a lower clean count.
    for i in 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16; do
      printf 'type SelftestNameBasedLookup10279_%s = HashMap<String, CoreTypeVar>;\n' "$i"
    done
  } >> "$1/subset_julia_vm_types/src/inference_core/type_core/match.rs"
}

# check_name_based_lookup.sh violation: replace one classified lexical
# TypeVar/CoreType binding with an unclassified raw name-keyed map. Before the
# classified alias lands, keep the legacy total unchanged to prove that the
# old count-only ratchet cannot detect one-for-one semantic substitution.
inject_unclassified_typevar_core_binding() {
  python3 - "$1/subset_julia_vm_types/src/inference_core/dispatch_resolver/core_match.rs" <<'PY'
import sys

p = sys.argv[1]
s = open(p).read()
classified = "LexicalTypeBindings"
raw = "HashMap<String, CoreType>"
if classified in s:
    s = s.replace(classified, raw, 1)
    s = s.replace(
        "use super::LexicalTypeBindings;",
        "use super::LexicalTypeBindings;\nuse std::collections::HashMap;",
        1,
    )
    s += "\n// audit-selftest SelftestUnclassifiedTypeVarCoreBinding10992\n"
else:
    if raw not in s:
        raise SystemExit("injector precondition missing: raw lexical binding map")
    s = s.replace(raw, "std::collections::BTreeMap<String, CoreType>", 1)
    s += "\n// audit-selftest SelftestUnclassifiedTypeVarCoreBinding10992\n"
    s += "type SelftestUnclassifiedTypeVarCoreBinding10992 = HashMap<String, CoreType>;\n"
open(p, "w").write(s)
PY
}

# check_name_based_lookup.sh violation: disconnect the Main-owner branch from
# the canonical scoped struct resolver while leaving the surrounding API and
# registry fields intact (Issue #11046).
inject_name_based_lookup_main_owner_disconnect() {
  python3 - "$1/subset_julia_vm_bytecode/src/struct_registry.rs" <<'PY'
import sys
p = sys.argv[1]
s = open(p).read()
old = "self.resolve_in_owner(MAIN_MODULE_PATH, name)"
if old not in s:
    raise SystemExit("injector precondition missing: Main-owner resolver delegation")
s = s.replace(old, "self.resolve(name) /* SelftestMainOwnerDisconnect11046 */", 1)
open(p, "w").write(s)
PY
}

# check_name_based_lookup.sh violation: make cache restoration derive a bare
# parametric declaration's owner from its display spelling again (Issue #11046).
inject_name_based_lookup_cache_owner_disconnect() {
  python3 - "$1/subset_julia_vm_compile/src/compile/cache.rs" <<'PY'
import sys
p = sys.argv[1]
s = open(p).read()
old = "struct_table.insert_owned("
if old not in s:
    raise SystemExit("injector precondition missing: cache owner-aware insertion")
s = s.replace(old, "struct_table.insert(", 1)
open(p, "w").write(s)
PY
}

# check_exception_taxonomy_funnel.sh violation R1 (Issue #11146): a raise site
# whose MESSAGE names one Julia exception class while the raised VARIANT is
# another — the exact shape behind 4 of the 5 root causes Issue #10354 found
# (`VmError::TypeError(format!("ArgumentError: ..."))`).
inject_exception_taxonomy_message_class() {
  {
    printf '%s\n' ''
    printf '%s\n' '// audit-selftest SelftestExceptionTaxonomyFunnel11146'
    printf '%s\n' 'fn selftest_exception_taxonomy_funnel_11146(name: &str) -> VmError {'
    printf '%s\n' '    VmError::TypeError(format!("ArgumentError: {}: bad arity", name))'
    printf '%s\n' '}'
  } >> "$1/subset_julia_vm_vm/src/vm/builtins_types.rs"
}

# check_exception_taxonomy_funnel.sh violation R2 (Issue #11146): the catch-time
# exception builder re-hard-codes a Julia exception struct-name literal instead
# of taking it from ExceptionClass::julia_name(), which is how a raise site could
# again bind a catch value whose class contradicts its variant.
inject_exception_taxonomy_hardcoded_name() {
  injector_python "$1" "subset_julia_vm_vm/src/vm/exec/error_handling.rs" <<'PY'
import pathlib
import sys
from audit_selftest_edit import replace_literal_exactly_once

path = pathlib.Path(sys.argv[1])
anchor = "        let fields: Vec<Value> = match err {"
inject = (
    "        // audit-selftest SelftestExceptionTaxonomyHardcodedName11146\n"
    '        let _selftest_name_11146 = "MethodError";\n'
)
replace_literal_exactly_once(
    path, anchor, inject + anchor, label="error-handling fields match"
)
PY
}

# check_exception_taxonomy_funnel.sh violation R3 (Issue #11146): a catch-all arm
# in the funnel's own match, which would let a new VmError variant be added
# WITHOUT declaring its Julia exception class — the ad-hoc-taxonomy hole itself.
inject_exception_taxonomy_catch_all() {
  injector_python "$1" "subset_julia_vm_bytecode/src/error.rs" <<'PY'
import pathlib
import sys
from audit_selftest_edit import replace_literal_exactly_once

path = pathlib.Path(sys.argv[1])
anchor = "            Self::ParseError(_) => ExceptionClass::ParseError,"
inject = anchor + (
    "\n            // audit-selftest SelftestExceptionTaxonomyCatchAll11146\n"
    "            _ => ExceptionClass::ErrorException,"
)
replace_literal_exactly_once(path, anchor, inject, label="VmError ParseError arm")
PY
}

# check_exception_taxonomy_funnel.sh violation R4 (Issue #11146): a NEW pure-Julia
# raise that names an exception class inside an `error("<Class>: ...")` message —
# an ErrorException whose message claims to be a BoundsError. The Julia-layer
# ratchet must only ever shrink.
inject_exception_taxonomy_julia_error_class() {
  {
    printf '%s\n' ''
    printf '%s\n' '# audit-selftest SelftestExceptionTaxonomyJuliaError11146'
    printf '%s\n' 'function selftest_exception_taxonomy_julia_11146(i)'
    printf '%s\n' '    error("BoundsError: selftest index $(i) out of range")'
    printf '%s\n' 'end'
  } >> "$1/subset_julia_vm/src/julia/base/some.jl"
}

# audit_exception_payload_carrier.sh violation (Issue #11647): reintroduce an
# independently owned typed payload slot beside the canonical one-shot carrier.
inject_exception_payload_ad_hoc_field() {
  injector_python "$1" "subset_julia_vm_vm/src/vm/mod.rs" <<'PY'
import pathlib
import sys
from audit_selftest_edit import replace_literal_exactly_once

path = pathlib.Path(sys.argv[1])
anchor = "    pending_exception_payload: exec::exception_payload::PendingExceptionPayloadCarrier,"
mutation = anchor + (
    "\n    // audit-selftest SelftestExceptionPayloadCarrier11647\n"
    "    pending_selftest_exception_payload: Option<Value>,"
)
replace_literal_exactly_once(
    path, anchor, mutation, label="canonical pending exception payload field"
)
PY
}

# Issue #11647: the exception funnel must drain the one-shot carrier before
# `julia_name()?` can return early for a VM-internal error.
inject_exception_payload_consume_after_classification() {
  injector_python "$1" "subset_julia_vm_vm/src/vm/exec/error_handling.rs" <<'PY'
import pathlib
import sys
from audit_selftest_edit import replace_literal_exactly_once

path = pathlib.Path(sys.argv[1])
old = (
    "        let pending_fields = self.pending_exception_payload.take_fields_for(err);\n"
    "        // The funnel decides the class; a VM-internal error has no Julia\n"
    "        // exception object at all and stays uncatchable.\n"
    "        let name = err.exception_class().julia_name()?;"
)
new = (
    "        // audit-selftest SelftestExceptionPayloadConsumeLate11647\n"
    "        let name = err.exception_class().julia_name()?;\n"
    "        let pending_fields = self.pending_exception_payload.take_fields_for(err);"
)
replace_literal_exactly_once(
    path, old, new, label="exception payload consume-before-classify order"
)
PY
}

# Issue #11647: prove the audit inventories Value-carrying Vm fields instead of
# relying only on names containing `pending_*error*payload`.
inject_exception_payload_name_bypass_field() {
  injector_python "$1" "subset_julia_vm_vm/src/vm/mod.rs" <<'PY'
import pathlib
import sys
from audit_selftest_edit import replace_literal_exactly_once

path = pathlib.Path(sys.argv[1])
anchor = "    pending_exception_payload: exec::exception_payload::PendingExceptionPayloadCarrier,"
mutation = anchor + (
    "\n    // audit-selftest SelftestExceptionPayloadNameBypass11647\n"
    "    deferred_typed_fields: Option<Value>,"
)
replace_literal_exactly_once(
    path, anchor, mutation, label="canonical pending exception payload field"
)
PY
}

# Issue #10436: signature alias exclusion and function-body type-parameter
# lookup must not regain independent lexical scope stacks.
inject_parallel_where_binder_stack() {
  {
    printf '%s\n' ''
    printf '%s\n' '// audit-selftest SelftestParallelWhereBinderStack10436'
    printf '%s\n' 'static EXCLUDED_PARAMS: () = ();'
  } >> "$1/subset_julia_vm_lowering/src/lowering/type_alias.rs"
}

# check_type_representation_string_reparse.sh violation: add a new semantic
# JuliaType::from_name call in an inference_core production root.
inject_type_representation_string_reparse() {
  {
    printf '%s\n' ''
    printf '%s\n' '// audit-selftest SelftestTypeStringReparse10460'
    printf '%s\n' 'fn selftest_type_string_reparse_10460() {'
    printf '%s\n' '    let _ = JuliaType::from_name(SelftestTypeStringReparse10460);'
    printf '%s\n' '}'
  } >> "$1/subset_julia_vm_types/src/inference_core/type_core/match.rs"
}

# Function-item aliases are semantic references even without a following `(`.
# The symbol-token scanner must reject this valid Rust spelling (Issue #10460).
inject_type_representation_string_reparse_alias() {
  {
    printf '%s\n' ''
    printf '%s\n' 'fn selftest_type_string_reparse_alias_10460() {'
    printf '%s\n' '    let SelftestTypeStringReparseAlias10460 = JuliaType::from_name;'
    printf '%s\n' '    let _ = SelftestTypeStringReparseAlias10460("Int64");'
    printf '%s\n' '}'
  } >> "$1/subset_julia_vm_types/src/inference_core/type_core/match.rs"
}

# Keep the aggregate count unchanged while replacing one reviewed site. The
# exact inventory digest must reject this substitution (Issue #10460).
inject_type_reparse_same_count_substitution() {
  injector_python "$1" "subset_julia_vm_vm/src/vm/builtins_types.rs" <<'PY'
import pathlib
import sys
from audit_selftest_edit import replace_literal_exactly_once

path = pathlib.Path(sys.argv[1])
replace_literal_exactly_once(
    path,
    "    crate::types::JuliaType::from_name(type_name)\n",
    "    crate::types::JuliaType::from_name(SelftestTypeReparseInventory10460)\n",
    label="reviewed type-reparse inventory site",
)
PY
}

# A legitimate-looking debt reduction must still require an explicit baseline
# and digest update; otherwise an under-baseline bucket can hide replacement
# sites indefinitely (Issue #10460).
inject_type_reparse_below_baseline_drift() {
  injector_python "$1" "subset_julia_vm_vm/src/vm/builtins_types.rs" <<'PY'
import pathlib
import sys
from audit_selftest_edit import replace_literal_exactly_once

path = pathlib.Path(sys.argv[1])
replace_literal_exactly_once(
    path,
    "    crate::types::JuliaType::from_name(type_name)\n",
    "    crate::types::JuliaType::Any // SelftestTypeReparseBelowBaseline10460\n",
    label="reviewed type-reparse site removal",
)
PY
}

# Issue #11208: restore the pre-#11207 state transition that cleared a pending
# cfg(test) marker on blank lines and adjacent attributes. The audit's focused
# matrix must fail before test-only tokens can perturb production baselines.
inject_type_reparse_cfg_test_trivia_regression() {
  injector_python "$1" "scripts/check_type_representation_string_reparse.sh" <<'PY'
import pathlib
import sys
from audit_selftest_edit import replace_literal_exactly_once

path = pathlib.Path(sys.argv[1])
anchor = '''            if not stripped or stripped.startswith("#["):
                out.append("")
                continue
            pending_cfg_test = False
'''
replacement = '''            pending_cfg_test = False  # SELFTEST11208-CFG-TRIVIA
'''
replace_literal_exactly_once(
    path, anchor, replacement, label="cfg(test) trivia transition"
)
PY
}

# check_base_duplicate_signatures.sh violation: add two same-signature bundled
# Base methods that are not classified in the allowlist.
inject_base_duplicate_signatures() {
  cat >> "$1/subset_julia_vm/src/julia/base/int.jl" <<'JL'

# audit-selftest selftest_duplicate_signature_10185
function selftest_duplicate_signature_10185(x)
    return x
end

function selftest_duplicate_signature_10185(y)
    return y
end
JL
}

# inventory_rust_semantics.sh parser-drift violation: add a `BuiltinId` enum
# variant with no matching `define_builtin_table!` row.
inject_inventory() {
  python3 - "$1/subset_julia_vm_bytecode/src/builtins.rs" <<'PY'
import re, sys
p = sys.argv[1]
s = open(p).read()
s, n = re.subn(r'(pub enum BuiltinId \{\n)',
               r'\1    SelftestBogusBuiltin9129,\n', s, count=1)
assert n == 1, "could not find `pub enum BuiltinId {`"
open(p, 'w').write(s)
PY
}

# check_value_array_allowlist.sh zero-match violation: reintroduce the retired
# `Value::Array` variant text in a source file. (The former ExprArgs confinement
# half was retired to a type in #8918; the only remaining policy is the
# deleted-variant zero-match guard.)
inject_value_array() {
  printf '%s\n' '// selftest9388 Value::Array(retired)' \
    >> "$1/subset_julia_vm_vm/src/vm/mod.rs"
}

# check_memory_to_array_ref_allowlist.sh zero-match violation: reintroduce the
# retired compatibility bridge call.
inject_memory_to_array_ref() {
  printf '%s\n' 'fn selftest9388_bridge() { memory_to_array_ref(0); }' \
    >> "$1/subset_julia_vm_vm/src/vm/mod.rs"
}

# check_complex_interleaved_allowlist.sh containment violation: a NON-allowlisted
# vm file gains the interleaved-Complex marker.
inject_complex_interleaved() {
  printf '%s\n' '// selftest9388 interleaved complex storage' \
    >> "$1/subset_julia_vm_vm/src/vm/builtins_io.rs"
}

# check_native_value_ops_resolve_structref.sh violation: membership no longer
# routes through values_equal_for_membership (rename every call in the type
# builtins so the audit's routing check fails).
inject_native_value_ops() {
  python3 - "$1/subset_julia_vm_vm/src/vm/builtins_types.rs" <<'PY'
import sys
p = sys.argv[1]
s = open(p).read()
s = s.replace("values_equal_for_membership(",
              "values_equal_for_membershipSELFTEST9388(")
open(p, 'w').write(s)
PY
}

# check_native_value_ops_resolve_structref.sh TYPE-WALL violation (Issue #8919):
# downgrade a structural compare/hash core sink from the `StructResolved` witness
# to raw `&Value` — an "un-witnessed sink" the type wall must reject.
inject_native_value_ops_witness() {
  python3 - "$1/subset_julia_vm_vm/src/vm/builtins_equality.rs" <<'PY'
import sys
p = sys.argv[1]
s = open(p).read()
s = s.replace(
    "fn egal_compare_witnessed(left: &StructResolved, right: &StructResolved)",
    "fn egal_compare_witnessed(left: &Value, right: &Value)",
    1,
)
open(p, 'w').write(s)
PY
}

# check_structural_debt_inventory.sh violation: reintroduce a stale closed-issue
# TODO reference (#1447), which the audit forbids unconditionally.
inject_structural_debt() {
  printf '%s\n' '// TODO(#1447) selftest marker9388 stale reference' \
    >> "$1/subset_julia_vm_vm/src/vm/mod.rs"
}

# check_definition_order_merges.sh violation: add a new raw Core-IR fragment
# transfer that has no chronology API call or inventory classification.
inject_definition_order_merge_bypass() {
  cat >> "$1/subset_julia_vm/src/pipeline.rs" <<'RS'

// audit-selftest SelftestDefinitionOrderBypass11036
fn selftest_definition_order_bypass_11036(program: &mut Program, module: crate::ir::core::Module) {
    program.modules.push(module);
}
RS
}

inject_definition_order_aliased_merge_bypass() {
  cat >> "$1/subset_julia_vm/src/pipeline.rs" <<'RS'

// audit-selftest SelftestDefinitionOrderAliasBypass11036
fn selftest_definition_order_alias_bypass_11036(program: &mut Program, module: crate::ir::core::Module) {
    let modules = &mut program.modules;
    modules.push(module);
}
RS
}

inject_definition_order_renamed_cursor_site() {
  cat >> "$1/subset_julia_vm/src/pipeline.rs" <<'RS'

// audit-selftest SelftestDefinitionOrderRenamedCursor11036
fn selftest_definition_order_renamed_cursor_11036(program: &mut Program) {
    let mut cursor = DefinitionOrderCursor::after_program(program);
    cursor.append_fragment(program);
}
RS
}

# check_definition_order_merges.sh violation: remove the runtime-nominal
# publication contract from the shared chronology/runtime-state inventory.
inject_definition_order_runtime_nominal_row_removed() {
  injector_python "$1" "docs/vm/DEFINITION_ORDER_MERGE_INVENTORY.tsv" <<'PY'
import pathlib
import re
import sys
from audit_selftest_edit import replace_regex_exactly_once

path = pathlib.Path(sys.argv[1])
replace_regex_exactly_once(
    path,
    r"^state\tsubset_julia_vm_vm/src/vm/mod.rs\t"
    r"runtime_nominals:runtime_nominal_activations\t[^\n]*\n",
    "# audit-selftest SelftestRuntimeNominalStateRowRemoved11740\n",
    label="runtime nominal state row",
    flags=re.MULTILINE,
)
PY
}

# check_callable_singleton_identity.sh violation: remove the FunctionValue
# authority accessor while leaving candidate/display fields intact.
inject_callable_singleton_identity_accessor_removed() {
  injector_python "$1" "subset_julia_vm_bytecode/src/value/metadata.rs" <<'PY'
import pathlib
import re
import sys
from audit_selftest_edit import replace_regex_exactly_once

path = pathlib.Path(sys.argv[1])
replace_regex_exactly_once(
    path,
    r"(impl FunctionValue \{.*?)(pub fn singleton_identity)"
    r"(\(&self\) -> &CallableSingletonIdentity \{)",
    r"\1pub fn singleton_identity_removed_11703\3",
    label="FunctionValue singleton identity accessor",
    flags=re.DOTALL,
)
PY
}

# check_rust_semantics_ratchet.sh violation: add a perf-pending classification
# row (baseline is 0). 4-column TSV shape so inventory's join still parses.
inject_rust_semantics_ratchet() {
  printf 'builtin\tSelftestPerfPending9388\tperf-pending\tselftest injected row (Issue #9388)\n' \
    >> "$1/docs/vm/rust_semantics_classification.tsv"
}

# check_numeric_matrix_full_allowlist.sh violation: repopulate the full matrix
# allowlist after milestone 62's zero-residual ratchet.
inject_numeric_matrix_full_allowlist() {
  printf 'selftest\tregression\t9849\t1\tselftest injected residual row (Issue #9849)\n' \
    >> "$1/docs/vm/NUMERIC_MATRIX_FULL_ALLOWLIST.tsv"
}

# check_generator_trait_matrix.sh violation: append one additional skiplist row
# using a real oracle id, proving the zero-residual ratchet fires before stale
# fixture regeneration can mask the intended failure reason (Issue #10325).
inject_generator_trait_matrix_skiplist_row_growth() {
  local first_id
  first_id="$(
    awk -F '\t' 'NR > 1 && $0 !~ /^[[:space:]]*$/ && $0 !~ /^#/ { print $1; exit }' \
      "$1/subset_julia_vm/tests/fixtures/generator/generator_trait_matrix_9566.tsv"
  )"
  if [ -z "$first_id" ]; then
    echo "no generator trait matrix oracle row available" >&2
    return 1
  fi
  printf '%s\t9566\tselftest\tSelftestGeneratorTraitSkiplistGrow9388 injected row (Issue #10325)\n' "$first_id" \
    >> "$1/docs/vm/GENERATOR_TRAIT_MATRIX_SKIPLIST.tsv"
}

# check_panic_free_ratchet.sh violation: a NEW source file (its own module key,
# baseline 0) with an .unwrap() call.
inject_panic_free_ratchet() {
  printf '%s\n' 'fn selftest_panic_ratchet_9388() { let _ = None::<i32>.unwrap(); }' \
    > "$1/subset_julia_vm/src/selftest_panic_ratchet_9388.rs"
}

# check_error_span_ratchet.sh violation: a NEW source file (its own module key,
# baseline 0) with a span-less Err(VmError::..).
inject_error_span_ratchet() {
  printf '%s\n' 'fn selftest_errspan_9388() { return Err(VmError::Selftest9388); }' \
    > "$1/subset_julia_vm/src/selftest_errspan_9388.rs"
}

# audit_compile_vm_coupling.sh violation: a new runtime vm -> compile import
# (vm_to_compile baseline 0).
inject_compile_vm_coupling() {
  printf '%s\n' 'use crate::compile::selftest9388;' \
    >> "$1/subset_julia_vm_vm/src/vm/mod.rs"
}

# audit_compile_vm_coupling.sh violation: a new compile-side test helper
# directly reaches into crate::vm. compile_to_vm intentionally has no test-only
# allowance, so this guards the Issue #9808 regression path where locally
# harmless compile tests bypassed the staged boundary.
inject_compile_test_vm_coupling() {
  cat >> "$1/subset_julia_vm_compile/src/compile/mod.rs" <<'RS'

#[cfg(test)]
mod selftest_9808_compile_to_vm {
    use crate::vm::Vm;

    #[test]
    fn selftest9808_compile_test_imports_vm() {
        let _ = core::mem::size_of::<Vm>();
    }
}
RS
}

# audit_base_cache_schema_fingerprint.sh violation: the schema manifest points
# at a file that does not exist (a rename/move that was not reflected — exactly
# the "silent breakage" this ratchet guards).
inject_base_cache_fingerprint() {
  printf '%s\n' 'src/selftest_missing_9388.rs' \
    >> "$1/subset_julia_vm_compile/src/compile/base_cache_schema_files.txt"
}

# Issue #10688 root-cause control: a real manifest-listed source changes while
# CACHE_VERSION and the committed snapshot stay untouched.
inject_base_cache_listed_file_drift() {
  printf '%s\n' '// SelftestBaseCacheListedFileDrift10688' \
    >> "$1/subset_julia_vm_compile/src/compile/instr_wire_ids.rs"
}

# check_call_handler_kwargs.sh violation: enough new inline kwparam loops to
# exceed the baseline (baseline 10, clean count 3 — inject 8).
inject_call_handler_kwargs() {
  for _ in 1 2 3 4 5 6 7 8; do
    printf '%s\n' '// selftest9388 for kwparam in &func.kwparams' \
      >> "$1/subset_julia_vm_vm/src/vm/exec/mod.rs"
  done
}

# check_audit_scripts_bash3_compat.sh violation: a copied check_*.sh gains a
# bash-4 associative-array declaration. The payload is assembled from parts so
# the literal `declare -A` does not appear in THIS script's source (which the
# compat audit itself scans).
inject_bash3_compat() {
  printf 'declare %s selftest9388=()\n' '-A' \
    >> "$1/scripts/check_div_specializations.sh"
}

# check_builtin_duplicates.sh violation: the same BuiltinId variant appears in
# two specialized handler files. The variant name has NO digits — the audit's
# `BuiltinId::[A-Za-z_]+` grep would otherwise truncate a numeric suffix.
inject_builtin_duplicates() {
  printf '%s\n' 'let _ = BuiltinId::SelftestDuplicateMarker;' \
    >> "$1/subset_julia_vm_vm/src/vm/builtins_io.rs"
  printf '%s\n' 'let _ = BuiltinId::SelftestDuplicateMarker;' \
    >> "$1/subset_julia_vm_vm/src/vm/builtins_arrays.rs"
}

# check_no_expect_in_bin.sh violation: a .expect() call in a bin/ crate.
inject_no_expect_in_bin() {
  printf '%s\n' 'fn selftest9388() { let _ = Some(0).expect("selftest9388"); }' \
    >> "$1/subset_julia_vm/src/bin/sjulia.rs"
}

# check_div_specializations.sh violation: reintroduce a concrete same-type div
# method that the BitSigned / BitUnsigned generic methods intentionally retired.
inject_div_specializations() {
  cat >> "$1/subset_julia_vm/src/julia/base/int.jl" <<'JL'

# audit-selftest selftest9388_concrete_div
function div(x::Int8, y::Int8)
    return x
end
JL
}

# check_promote_builtin_no_tuple_fallback.sh violation: reintroduce the old
# silent unchanged-tuple fallback after Julia promote method lookup misses.
inject_promote_builtin_tuple_fallback() {
  injector_python "$1" "subset_julia_vm_vm/src/vm/builtins_exec.rs" <<'PY'
import pathlib
import sys
from audit_selftest_edit import replace_literal_exactly_once

path = pathlib.Path(sys.argv[1])
needle = '                let arg_type_names = values\n'
replacement = (
    '                // audit-selftest selftest9896_promote_tuple_fallback\n'
    '                self.stack.push(Value::Tuple(values.clone()));\n'
    + needle
)
replace_literal_exactly_once(path, needle, replacement, label="promote miss path")
PY
}

# check_array_constructor_memory_first.sh violation: open-code a typed undef
# array allocation in a pinned constructor file.
inject_array_constructor_memory_first() {
  printf '%s\n' 'fn selftest9388() { let _ = ArrayValue::undef_typed(0); }' \
    >> "$1/subset_julia_vm_vm/src/vm/builtins_arrays.rs"
}

# check_no_hardcoded_var_names_in_inference.sh violation: enough new hardcoded
# name == "X" struct checks to exceed the baseline (baseline 9, clean count 7 —
# inject 4).
inject_no_hardcoded_var_names() {
  for _ in 1 2 3 4; do
    printf '%s\n' 'fn selftest9388() { if name == "SelftestStruct9388" {} }' \
      >> "$1/subset_julia_vm_compile/src/compile/expr/infer/array.rs"
  done
}

# check_no_public_base_stdlib_routes.sh violation: a direct "Base.<stdlib>"
# string route in compile code.
inject_no_public_base_stdlib() {
  printf '%s\n' 'fn selftest9388() { let _ = "Base.Dates.selftest9388"; }' \
    >> "$1/subset_julia_vm_compile/src/compile/core_compiler.rs"
}

# check_generated_files.sh violation: a new @generated file with no
# "Re-generate with:" comment.
inject_generated_files() {
  printf '// @generated selftest9388\nfn selftest_generated_9388() {}\n' \
    > "$1/subset_julia_vm/src/selftest_generated_9388.rs"
}

# check_build_preload_packages_explicit.sh violation (Issue #11055): make the
# unset default non-empty, which would generate/embed a preload cache without
# the caller explicitly setting SJULIA_PRELOAD_PACKAGES.
inject_build_preload_packages_explicit() {
  injector_python "$1" "build.sh" <<'PY'
import pathlib
import sys
from audit_selftest_edit import replace_literal_exactly_once

path = pathlib.Path(sys.argv[1])
needle = '    PRELOAD_PACKAGES_FOR_BUILD=""\n'
replacement = (
    '    # audit-selftest selftest11055_implicit_preload_default\n'
    '    PRELOAD_PACKAGES_FOR_BUILD="Plots"\n'
)
replace_literal_exactly_once(path, needle, replacement, label="explicit preload default")
PY
  assignment="$(grep -F '    PRELOAD_PACKAGES_FOR_BUILD="Plots"' "$1/build.sh" | head -1)"
  bash -u -c "$assignment
test \"\$PRELOAD_PACKAGES_FOR_BUILD\" = Plots"
}

# check_build_locked.sh violation (Issue #11257): add an unlocked cargo build
# split across a shell continuation. The audit must join the logical command
# portably and reject it on both BSD/macOS and GNU/Linux userlands.
inject_build_locked_multiline_unlocked() {
  cat >> "$1/scripts/check_build_locked.sh" <<'EOF'
# audit-selftest SelftestBuildLockedMultiline11257
cargo build \
  --release
EOF
}

# --------------------------------------------------------------------------
# Injections added in Issue #9463 — graduating the remaining injectable
# static-grep audits from NO_SELFTEST_REASONS into real negative controls.
# Each carries a `selftest9463` marker so the reason cannot collide with real
# source or with the 9129/9388 injections above.
# --------------------------------------------------------------------------

# check_array_literal_memory_first.sh violation: open-code a TypedArrayValue in
# the pinned array-literal builder file.
inject_array_literal_memory_first() {
  printf '%s\n' 'fn selftest9463() { let _ = TypedArrayValue::new(); }' \
    >> "$1/subset_julia_vm_vm/src/vm/exec/array_basic.rs"
}

# check_broadcast_hof_memory_first.sh violation: an open-coded ArrayValue::from_f64
# result builder in a broadcast/HOF file.
inject_broadcast_hof_memory_first() {
  printf '%s\n' 'fn selftest9463() { let _ = ArrayValue::from_f64(vec![]); }' \
    >> "$1/subset_julia_vm_vm/src/vm/broadcast.rs"
}

# check_collect_memory_first.sh violation: materialize collect(tuple) via a
# non-Memory-first ArrayValue helper in the iteration target.
inject_collect_memory_first() {
  printf '%s\n' 'fn selftest9463() { let _ = ArrayValue::from_i64(vec![]); }' \
    >> "$1/subset_julia_vm_vm/src/vm/type_ops/iteration.rs"
}

# check_collect_memory_first.sh violation (Issue #9573 repoint): materialize
# collect(range) via a non-Memory-first ArrayValue helper in Range::collect's
# current home. The check's old `vm/value/range.rs` target drifted in the
# #8655/#8656 crate split (→ subset_julia_vm_bytecode/src/value/range.rs) and
# was a silent no-op until repointed.
inject_collect_range_memory_first() {
  printf '%s\n' 'fn selftest9573() { let _ = ArrayValue::from_i64(vec![]); }' \
    >> "$1/subset_julia_vm_bytecode/src/value/range.rs"
}

# check_collect_memory_first.sh moved-target guard (Issue #9573): rename a
# hardcoded target file out from under the audit. The path guard must fail
# loudly ("audit target file missing"), instead of the pre-#9573 behavior where
# a grep over the absent path silently reported OK (failure mode F2, #9129).
inject_collect_memory_first_moved_target() {
  mv "$1/subset_julia_vm_bytecode/src/value/range.rs" \
    "$1/subset_julia_vm_bytecode/src/value/range_selftest9573_moved.rs"
  printf '%s\n' '// selftest9573moved' \
    >> "$1/subset_julia_vm_bytecode/src/value/range_selftest9573_moved.rs"
}

# check_literal_repl_memory_first.sh violation: a non-Memory-first Literal::Array*
# conversion helper.
inject_literal_repl_memory_first() {
  printf '%s\n' 'fn selftest9463() { let _ = ArrayValue::from_f64(vec![]); }' \
    >> "$1/subset_julia_vm_compile/src/compile/expr/mod.rs"
}

# check_base_routing_registry.sh violation: a BASE_FUNCTION_ROUTES entry with an
# empty upstream_ref (the early global-grep guard, before the doc-inventory
# checks, so it fires in the source-only sandbox).
inject_base_routing_registry() {
  printf '%s\n' 'route(selftest9463, x, y, "")' \
    >> "$1/subset_julia_vm_compile/src/compile/base_functions.rs"
}

# check_no_new_domain_builtins.sh violation: add an unjustified BuiltinId
# variant so the hard count ratchet fires with an injection-specific reason
# (the injected variant name in the audit's variant listing — a clean tree can
# never print it, whatever the drifted count is).
inject_no_new_domain_builtins() {
  python3 - "$1/subset_julia_vm_bytecode/src/builtins.rs" <<'PY'
import re, sys
p = sys.argv[1]
s = open(p).read()
s, n = re.subn(r'(pub enum BuiltinId \{\n)',
               r'\1    SelftestDomainBuiltin9463,\n', s, count=1)
assert n == 1, "could not find `pub enum BuiltinId {`"
open(p, 'w').write(s)
PY
}

# check_no_new_domain_builtins.sh Layer-2 LOC ratchet violation (Issue #9892):
# a NEW builtins_*.rs file that ALONE exceeds baseline+tolerance (constants read
# from the sandboxed audit script), so the ratchet trips regardless of how the
# rest of the tree has drifted in either direction. The audit prints a per-file
# LOC breakdown on LOC-ratchet failure, so the injected filename is the
# injection-specific reason — the generic "Layer-2 LOC grew" message also
# appears when the CLEAN tree has drifted past the ceiling and must not be
# matched.
inject_no_new_domain_builtins_loc() {
  local audit="$1/scripts/check_no_new_domain_builtins.sh"
  local base tol lines f
  base="$(sed -n 's/^BASELINE_BUILTIN_LOC=\([0-9][0-9]*\)$/\1/p' "$audit")"
  tol="$(sed -n 's/^LOC_TOLERANCE=\([0-9][0-9]*\)$/\1/p' "$audit")"
  lines=$(( ${base:-20000} + ${tol:-300} + 1 ))
  f="$1/subset_julia_vm_vm/src/vm/builtins_selftest9892loc.rs"
  awk -v n="$lines" 'BEGIN { for (i = 0; i < n; i++) print "// selftest9892loc filler" }' > "$f"
}

# check_unsafe_inventory.sh violation: a NEW unannotated `unsafe { }` site
# (fresh fingerprint absent from UNSAFE_INVENTORY_BASELINE.tsv, no Safety:
# comment).
inject_unsafe_inventory() {
  printf '%s\n' 'fn selftest9463unsafe() { unsafe { let _ = 1; } }' \
    >> "$1/subset_julia_vm_vm/src/vm/mod.rs"
}

# check_workarounds_documented.sh violation: a `// Workaround:` comment with no
# `(Issue #NNNN)` link.
inject_workarounds_documented() {
  printf '%s\n' '// Workaround: selftest9463 undocumented shortcut' \
    >> "$1/subset_julia_vm_vm/src/vm/mod.rs"
}

# check_workarounds_sync.sh violation: a `// Workaround: ... (Issue #NNNN)`
# comment whose Issue number is absent from docs/vm/WORKAROUNDS.md.
inject_workarounds_sync() {
  printf '%s\n' '// Workaround: selftest9463 sync gap (Issue #99999463)' \
    >> "$1/subset_julia_vm_vm/src/vm/mod.rs"
}

# audit_julia_base_stubs.sh violation: an unmarked trivial-body untyped helper in
# an upstream-swept Pure Julia file. The line immediately after `function` must
# be the trivial `return`.
inject_julia_base_stubs() {
  printf '%s\n%s\n%s\n' 'function selftest9463stub(x)' '    return true' 'end' \
    >> "$1/subset_julia_vm/src/julia/base/bool.jl"
}

# check_missing_debug.sh violation: a public struct in compile/ with no
# `#[derive(Debug)]`. The audit echoes the offending type name.
inject_missing_debug() {
  printf '%s\n%s\n%s\n' 'pub struct Selftest9463NoDebug {' '    field: i32,' '}' \
    >> "$1/subset_julia_vm_compile/src/compile/core_compiler.rs"
}

# check_array_public_data_access.sh violation: a raw `try_data_f64()?` read in
# broadcast.rs that is NOT the exempt interleaved-complex indexing form. The
# audit echoes the offending line (carrying the unique marker).
inject_array_public_data_access() {
  printf '%s\n' 'let _v = arr.try_data_f64()?; // selftest9463arraypub' \
    >> "$1/subset_julia_vm_vm/src/vm/broadcast.rs"
}

# check_array_public_data_access.sh violation (Issue #9573): drop the
# shared-parent classification anchor from the repointed
# subset_julia_vm_bytecode access.rs target, so the presence check
# (`! rg -q "if let Some(parent) = &self.shared_parent"`) fires.
inject_array_shared_parent_anchor() {
  injector_python "$1" "subset_julia_vm_bytecode/src/value/array_value/access.rs" <<'PY'
import pathlib
import re
import sys
from audit_selftest_edit import replace_regex_exactly_once

path = pathlib.Path(sys.argv[1])
anchor = "if let Some(parent) = &self.shared_parent"

def remove_shared_parent_checks(match):
    owner = match.group(0)
    count = len(re.findall(re.escape(anchor), owner))
    if count != 3:
        raise AssertionError(
            f"shared-parent classification: expected three owner checks, found {count}"
        )
    return re.sub(
        re.escape(anchor),
        "if let Some(parent) = &self.selftest9573_parent",
        owner,
    )

replace_regex_exactly_once(
    path,
    r"pub fn get\(&self,.*?(?=    pub fn get_linear_f64\()",
    remove_shared_parent_checks,
    label="shared-parent classification",
    flags=re.DOTALL,
)
PY
}

# check_array_public_data_access.sh violation: a public Value::Generator
# getindex branch that materializes a value instead of raising MethodError.
inject_generator_public_indexing_materialization() {
  local f="$1/subset_julia_vm_vm/src/vm/exec/array_index.rs"
  local tmp="$f.tmp"
  awk '
      { print }
      /target @ Value::Generator\(_\) => \{/ && !done {
          print "                            self.stack.push(Value::Nothing); // selftest9735genindex"
          done = 1
      }
  ' "$f" > "$tmp"
  mv "$tmp" "$f"
}

# check_binary_both_fallback_inventory.sh violation: a `BinaryBothFallback:` tag
# in code with no matching entry in docs/vm/BINARY_DISPATCH.md.
inject_binary_both_fallback() {
  printf '%s\n' '// BinaryBothFallback: Selftest9463bbtag' \
    >> "$1/subset_julia_vm_vm/src/vm/exec/binary_both.rs"
}

# check_collect_fallback_inventory.sh violation: a `CollectFallback:` tag in code
# with no matching entry in docs/vm/COLLECTIONS.md.
inject_collect_fallback() {
  printf '%s\n' '// CollectFallback: Selftest9463cftag' \
    >> "$1/subset_julia_vm_vm/src/vm/builtins_exec.rs"
}

# check_vmerror_classification.sh violation: a NEW vm/exec file with a block of
# unannotated `return Err(VmError::TypeError(...))` — enough to exceed the
# baseline (49) regardless of the current drifted count. The audit lists the
# offending file:line, so the unique filename is the injection-specific reason.
inject_vmerror_classification() {
  local f="$1/subset_julia_vm_vm/src/vm/exec/selftest9463_vmerr.rs"
  local _
  : > "$f"
  for _ in $(seq 1 60); do
    printf '%s\n' 'fn selftest9463_vmerr() { return Err(VmError::TypeError("selftest9463".into())); }' >> "$f"
  done
}

# check_no_panic_in_tests.sh violation: a NEW src file with a block of unannotated
# `=> panic!` match arms — enough to exceed SRC_BASELINE (84). tests/ is excluded
# from the sandbox, so only the src/ ratchet is exercised.
inject_no_panic_in_tests() {
  local f="$1/subset_julia_vm/src/selftest9463_panic.rs"
  local _
  : > "$f"
  for _ in $(seq 1 100); do
    printf '%s\n' 'fn selftest9463_panic() { match x { _ => panic!("selftest9463") } }' >> "$f"
  done
}

# audit_binary_dispatch_single_source.sh violation: remove the resolver-adapter
# coverage anchor for Add by renaming every `resolver_overrides_to_builtin`
# occurrence so the presence grep no longer matches.
inject_binary_dispatch_single_source() {
  python3 - "$1/subset_julia_vm_compile/src/compile/expr/binary/mod.rs" <<'PY'
import sys
p = sys.argv[1]
s = open(p).read()
assert "resolver_overrides_to_builtin" in s, "anchor token not present to remove"
s = s.replace("resolver_overrides_to_builtin", "resolverADAPTER9463removed")
open(p, "w").write(s)
PY
}

# audit_binary_dispatch_single_source.sh violation (Issue #9573 repoint): remove
# the `pub enum BinaryStaticVerdict` declaration anchor from dispatch_resolver.rs
# in its current home (moved to subset_julia_vm_types in the crate split; the
# old subset_julia_vm/src/inference_core path left these anchors red on main).
inject_binary_dispatch_resolver_anchor() {
  python3 - "$1/subset_julia_vm_types/src/inference_core/dispatch_resolver.rs" <<'PY'
import sys
p = sys.argv[1]
s = open(p).read()
assert "pub enum BinaryStaticVerdict" in s, "anchor token not present to remove"
s = s.replace("pub enum BinaryStaticVerdict", "pub enum SelftestVerdict9573", 1)
open(p, "w").write(s)
PY
}

# check_call_function_variable_value_dispatch_order.sh violation (Issues
# #9987/#10461): insert a local legacy scorer in the
# `Instr::CallFunctionVariable` arm, bypassing the shared semantic resolver.
inject_call_function_variable_dispatch_order() {
  injector_python "$1" "subset_julia_vm_vm/src/vm/exec/call_function_variable.rs" <<'PY'
import pathlib
import re
import sys
from audit_selftest_edit import replace_regex_exactly_once

path = pathlib.Path(sys.argv[1])
pattern = re.compile(
    r"(?m)^(?P<indent>[ \t]*)Instr::CallFunctionVariable\([^\n]*\)\s*=>\s*\{$"
)

def inject(match):
    indent = match.group("indent") + "    "
    legacy = (
        f"\n{indent}let selftest9987_local_legacy = self.dispatch_function_variable("
        '"selftest", &[], &[]);'
    )
    return match.group(0) + legacy

replace_regex_exactly_once(
    path, pattern, inject, label="CallFunctionVariable arm"
)
PY
}

# The positive call detector must ignore a comment that merely names the
# shared resolver. Remove the real call from the plain opcode arm while
# leaving that token in a comment (Issue #10461).
inject_call_function_variable_fake_shared_comment() {
  injector_python "$1" "subset_julia_vm_vm/src/vm/exec/call_function_variable.rs" <<'PY'
import pathlib
import re
import sys
from audit_selftest_edit import replace_regex_exactly_once

path = pathlib.Path(sys.argv[1])
pattern = re.compile(
    r"(?ms)(?P<prefix>^            Instr::CallFunctionVariable\([^\n]*\)\s*=>\s*\{.*?)(?P<indent>^[ \t]*)(?P<before>[^\n]*?)self\.dispatch_function_variable_for_values\("
)

def inject(match):
    indent = match.group("indent")
    return (
        match.group("prefix")
        + indent
        + "// self.dispatch_function_variable_for_values( SELFTEST10461-COMMENT\n"
        + indent
        + match.group("before")
        + "self.dispatch_function_variable("
    )

replace_regex_exactly_once(
    path, pattern, inject, label="real shared resolver call in CallFunctionVariable arm"
)
PY
}

# Direct dynamic calls must use the callee identity carried by bytecode, not
# reconstruct a spelling from candidate order (Issue #10461).
inject_call_dynamic_callee_identity_ignored() {
  injector_python "$1" "subset_julia_vm_vm/src/vm/exec/call_dynamic.rs" <<'PY'
import pathlib
import sys
from audit_selftest_edit import replace_literal_exactly_once

path = pathlib.Path(sys.argv[1])
replace_literal_exactly_once(
    path,
    "CalleeIdentity::from_function_name(&operands.callee_name)",
    'CalleeIdentity::from_function_name("selftest10461-anonymous")',
    label="CallDynamic explicit callee identity",
)
PY
}

# `invoke` must dispatch on its declared tuple without switching to the
# ordinary value-driven resolver (Issue #11619).
inject_invoke_declared_signature_runtime_refinement() {
  injector_python "$1" "subset_julia_vm_vm/src/vm/exec/call_function_variable.rs" <<'PY'
import pathlib
import sys
from audit_selftest_edit import replace_literal_exactly_once

path = pathlib.Path(sys.argv[1])
replace_literal_exactly_once(
    path,
    "self.dispatch_function_variable(func_name, &origin_compatible, declared_arg_type_names)",
    "self.dispatch_function_variable_for_values(func_name, &origin_compatible, declared_arg_type_names, args).and_then(|selected| selected.ok_or_else(|| VmError::MethodError(\"selftest11619 runtime refinement\".to_string())))",
    label="invoke declared-signature dispatch authority",
)
PY
}

# check_compile_expr_local_shadow_guard.sh violation (Issues #10044, #11269): insert an
# unguarded bare-name special-case at the top of the `Expr::Var(name, _)` arm,
# BEFORE the local-shadow guards — the exact regression shape of bug #10034
# (a keyword parameter named `stderr` was compiled as `PushStderr`).
inject_compile_expr_local_shadow_guard() {
  injector_python "$1" "subset_julia_vm_compile/src/compile/expr/mod.rs" <<'PY'
import pathlib
import re
import sys
from audit_selftest_edit import replace_regex_exactly_once

path = pathlib.Path(sys.argv[1])
pattern = re.compile(r"(?m)^ {12}Expr::Var\((?P<name>\w+),\s*[^)]*\)\s*=>\s*\{\n")

def inject(match):
    name = match.group("name")
    return match.group(0) + (
        f'                if {name} == "selftest10044" {{\n'
        "                    self.emit(Instr::PushNothing);\n"
        "                    return Ok(ValueType::Nothing);\n"
        "                }\n"
    )

replace_regex_exactly_once(path, pattern, inject, label="Expr::Var compile arm")
PY
}

# Issue #11604: deleting the InternedStr projection from the audit's accepted
# guard grammar must fail its production-independent conformance matrix with a
# projection-specific diagnostic.
inject_compile_expr_local_shadow_guard_projection_removed() {
  injector_python "$1" "scripts/check_compile_expr_local_shadow_guard.sh" <<'PY'
import pathlib
import sys
from audit_selftest_edit import replace_literal_exactly_once

path = pathlib.Path(sys.argv[1])
replace_literal_exactly_once(
    path,
    'NAME_ARG = r"name(?:\\s*\\.\\s*as_str\\s*\\(\\s*\\))?"',
    'NAME_ARG = r"name"  # SELFTEST11604-PROJECTION-REMOVED',
    label="local-shadow guard grammar projection",
)
PY
}

# check_specializer_callee_guard.sh violation (Issue #10418): insert a
# name-keyed callee comparison in the runtime specializer's compile_call
# BEFORE the front-door local-callee guard — the exact regression shape of
# Issue #10146 (a parameter named `Float64` compiled as the builtin
# constructor inside specialized bodies). Include the guard's explanatory
# comment in the anchor so the defensive copy in resolve_callable_callee does
# not make the injection ambiguous (Issue #10895).
inject_specializer_callee_guard() {
  injector_python "$1" "subset_julia_vm_vm/src/vm/specialize/expr.rs" <<'PY'
import pathlib
import re
import sys
from audit_selftest_edit import replace_regex_exactly_once

path = pathlib.Path(sys.argv[1])
pattern = re.compile(
    r"(?ms)(?P<owner>^[ \t]*pub\(super\) fn compile_call\(\n"
    r"[ \t]*&mut self,\n[ \t]*(?P<callee>[A-Za-z_]\w*):\s*&str,.*?"
    r"^(?P<indent>[ \t]*))(?P<guard>if self\.locals\.contains_key\("
    r"(?P=callee)\)\s*\{\n)"
)

def inject(match):
    indent = match.group("indent")
    callee = match.group("callee")
    mutation = (
        f'{indent}if {callee} == "selftest10418" {{\n'
        f"{indent}    return Err(SpecializationError::Unsupported(\n"
        f'{indent}        "selftest10418".to_string(),\n'
        f"{indent}    ));\n"
        f"{indent}}}\n"
    )
    return match.group("owner") + mutation + match.group("guard")

replace_regex_exactly_once(path, pattern, inject, label="specializer compile_call local guard")
PY
}

# check_lambda_context_routing.sh violation (Issues #10936/#10965): reintroduce
# a narrow `contains_macro_call`-style predicate at a function-lowering dispatch
# site outside the routing authority — the exact regression shape of Issue
# #10934 (a dispatch surface consulting a narrow predicate and dropping the
# where-binder edge).
inject_lambda_context_routing() {
  injector_python "$1" "subset_julia_vm_lowering/src/lowering/stmt/mod.rs" <<'PY'
import pathlib
import sys
from audit_selftest_edit import replace_literal_exactly_once

path = pathlib.Path(sys.argv[1])
anchor = "fn lower_stmt_impl<'a>("
injected = (
    "fn selftest10936_routing(x: bool) -> bool {\n"
    "    x && crate::lowering::contains_macro_call_selftest10936()\n"
    "}\n"
) + anchor
replace_literal_exactly_once(path, anchor, injected, label="lower_stmt_impl owner")
PY
}

# check_lambda_context_routing.sh R3 violation (Issues #11179/#11193): the
# LAUNDERING-WRAPPER shape. R2 only greps OUTSIDE the authority file, so it can
# be satisfied by relocating a context-free lowering call INTO
# `lowering/mod.rs` behind a pass-through wrapper and calling that wrapper from
# the dispatch site: the grep stops matching while `requires_lambda_context` is
# never consulted. This exact shim landed on main (commit d0dfe0578) and turned
# the audit green while struct-body `global` helpers silently lost their
# macro / where-binder / parametric context — a user macro in such a body could
# not be lowered at all. Re-inject that shim shape verbatim; R3 must reject it.
inject_lambda_context_routing_wrapper_shim() {
  injector_python "$1" "subset_julia_vm_lowering/src/lowering/mod.rs" <<'PY'
import pathlib
import re
import sys
from audit_selftest_edit import replace_regex_exactly_once

path = pathlib.Path(sys.argv[1])
pattern = re.compile(r"(?m)^fn lower_stmt_with_macro_ctx_if_needed<'a>\(")
mutation = (
    "pub(crate) fn selftest11179_struct_global_function_all<'a>(\n"
    "    walker: &CstWalker<'a>,\n"
    "    node: Node<'a>,\n"
    ") -> LowerResult<Vec<Function>> {\n"
    "    function::lower_function_all(walker, node)\n"
    "}\n\n"
)
replace_regex_exactly_once(
    path,
    pattern,
    lambda match: mutation + match.group(0),
    label="lower_stmt_with_macro_ctx_if_needed owner",
)
PY
}

# check_lambda_context_routing.sh R4 violation (Issue #11211): reintroduce the
# post-hoc watermark stamping shape that loses authority for functions drained
# into side lists before the slice is revisited. Creation-time stamping in
# LambdaContext::add_lifted_function must remain the sole lifted-function seam.
inject_lambda_context_posthoc_struct_new_stamp() {
  injector_python "$1" "subset_julia_vm_lowering/src/lowering/mod.rs" <<'PY'
import pathlib
import re
import sys
from audit_selftest_edit import replace_regex_exactly_once

path = pathlib.Path(sys.argv[1])
injected = (
    "pub(crate) fn selftest11211_posthoc_struct_new_stamp(\n"
    "    ctx: &LambdaContext,\n"
    "    start: usize,\n"
    "    struct_name: &str,\n"
    ") {\n"
    "    for mut func in ctx.lifted_functions_from_index(start) {\n"
    "        func.new_struct_name = Some(struct_name.to_string()); // SELFTEST11211-POSTHOC\n"
    "    }\n"
    "}\n\n"
)
replace_regex_exactly_once(
    path,
    re.compile(r"(?m)^fn lower_stmt_with_macro_ctx_if_needed<'a>\("),
    lambda match: injected + match.group(0),
    label="lower_stmt_with_macro_ctx_if_needed owner",
)
PY
}

# --------------------------------------------------------------------------
# run_selftest <label> <audit-basename> <reason-substring> <inject-fn>
#              <marker> <marker-relpath> [audit args...]
#
#   * baseline: run the audit on the CLEAN template. The reason MUST be absent
#     (otherwise it is not injection-specific — a mis-designed self-test). If the
#     clean tree exits 0, that is also reported as a positive control.
#   * negative control: copy the template, run <inject-fn>, verify <marker>
#     landed in <marker-relpath>, run the audit. It must exit non-zero AND print
#     <reason-substring> (case-insensitive, fixed-string). Exit 0 => the audit is
#     BROKEN (F2). Non-zero without the reason => it failed for an unrelated
#     cause, not by detecting the violation.
# --------------------------------------------------------------------------
target_selected() {
  local wanted="$1" candidate
  while IFS= read -r candidate; do
    [ -n "$candidate" ] || continue
    [ "$candidate" = "$wanted" ] && return 0
  done <<EOF
$TARGET_PATHS
EOF
  return 1
}

selftest_framework_changed() {
  target_selected "scripts/check_audit_negative_selftest.sh" ||
    target_selected "scripts/audit_selftest_edit.py" ||
    target_selected "docs/vm/AUDIT_SELFTEST_ANCHORS.tsv"
}

run_selftest() {
  local label="$1" audit="$2" reason="$3" inject_fn="$4" marker="$5" marker_file="$6"
  shift 6
  local out code
  COVERED_LIST="$COVERED_LIST$audit "
  TARGET_ROWS="$TARGET_ROWS$marker_file	$audit	$label
scripts/$audit	$audit	$label
"

  if [ "$LIST_TARGETS" -eq 1 ]; then
    return
  fi

  # Fast merge-time mode: executing the real top-level run_selftest calls is
  # the registration parser. Do not build sandboxes or run any audit here.
  if [ "$REGISTRATION_ONLY" -eq 1 ]; then
    return
  fi

  # Changes to the harness, shared edit helper, or anchor inventory can affect
  # every registered control, so their bounded premerge selection is the full
  # matrix. Ordinary source changes remain limited to their marker target.
  if [ "$FILTER_ACTIVE" -eq 1 ] &&
     ! selftest_framework_changed &&
     ! target_selected "$marker_file" &&
     ! target_selected "scripts/$audit"; then
    return
  fi
  SELECTED_COUNT=$((SELECTED_COUNT + 1))

  # 1. Baseline on the pristine template (read-only).
  out="$(cd "$TEMPLATE_SANDBOX" && SJULIA_UPSTREAM_JULIA=/nonexistent bash "scripts/$audit" "$@" 2>&1)"
  code=$?
  if printf '%s' "$out" | grep -qiF -- "$reason"; then
    bad "$label: reason '$reason' already appears on the CLEAN sandbox (exit $code) — it is not injection-specific. Pick a reason only the injection produces."
    return
  fi
  if [ "$code" -eq 0 ]; then
    pass "$label: clean tree passes (exit 0)"
  else
    log "      NOTE: $label is non-zero on the clean tree (exit $code) — likely baseline drift (#8740); validating injection-specific detection only."
  fi

  # 2. Negative control in an isolated copy.
  CURRENT_TEST_SANDBOX="$(mktemp -d)"
  cp -R "$TEMPLATE_SANDBOX"/. "$CURRENT_TEST_SANDBOX"/
  if ! "$inject_fn" "$CURRENT_TEST_SANDBOX"; then
    bad "$label: injection function '$inject_fn' failed — self-test could not create a valid bad input."
    rm -rf "$CURRENT_TEST_SANDBOX"; CURRENT_TEST_SANDBOX=""
    return
  fi
  if ! grep -qF -- "$marker" "$CURRENT_TEST_SANDBOX/$marker_file" 2>/dev/null; then
    bad "$label: injection did not apply (marker '$marker' absent in $marker_file) — self-test could not run."
    rm -rf "$CURRENT_TEST_SANDBOX"; CURRENT_TEST_SANDBOX=""
    return
  fi
  out="$(cd "$CURRENT_TEST_SANDBOX" && SJULIA_UPSTREAM_JULIA=/nonexistent bash "scripts/$audit" "$@" 2>&1)"
  code=$?
  rm -rf "$CURRENT_TEST_SANDBOX"; CURRENT_TEST_SANDBOX=""

  if [ "$code" -eq 0 ]; then
    bad "$label: audit PASSED on injected-bad input (exit 0) — the audit is BROKEN: it no longer guards its invariant (F2). Output:"
    printf '%s\n' "$out" | sed 's/^/      /'
    return
  fi
  if ! printf '%s' "$out" | grep -qiF -- "$reason"; then
    bad "$label: audit failed (exit $code) but WITHOUT the expected reason '$reason' — likely failing for an unrelated cause (moved path / drift), not by detecting the injection. Output:"
    printf '%s\n' "$out" | sed 's/^/      /'
    return
  fi
  pass "$label: injected violation detected (exit $code, reason matched '$reason')"
}

# Exercise check_build_locked.sh's input-normalization contract directly. These
# cases need both positive and negative assertions, so they cannot be expressed
# solely through run_selftest's injected-violation shape (Issue #11257).
run_build_locked_contract_matrix() {
  local audit="$REPO_ROOT/scripts/check_build_locked.sh"
  local case_dir file out code synthetic_root path
  case_dir="$(mktemp -d)"

  build_locked_expect() {
    local label="$1" expected="$2" reason="$3"
    shift 3
    out="$(bash "$audit" "$@" 2>&1)"
    code=$?
    if [ "$code" -ne "$expected" ]; then
      bad "check_build_locked.sh contract: $label exited $code, expected $expected. Output: $out"
    elif [ -n "$reason" ] && ! printf '%s' "$out" | grep -qF -- "$reason"; then
      bad "check_build_locked.sh contract: $label omitted diagnostic '$reason'. Output: $out"
    else
      pass "check_build_locked.sh contract: $label"
    fi
  }

  file="$case_dir/one-locked.sh"
  printf '%s\n' "cargo build \\" '  --locked' > "$file"
  build_locked_expect "one trailing backslash joins a locked continuation" 0 "" "$file"

  file="$case_dir/one-unlocked.sh"
  printf '%s\n' "cargo build \\" '  --release' > "$file"
  build_locked_expect "one trailing backslash joins an unlocked continuation" 1 "MISSING --locked:" "$file"

  file="$case_dir/two-no-smuggle.sh"
  printf '%s\n' "cargo build \\\\" '  --locked' > "$file"
  build_locked_expect "two trailing backslashes cannot smuggle a following --locked" 1 "MISSING --locked:" "$file"

  file="$case_dir/two-locked.sh"
  printf '%s\n' "cargo build --locked \\\\" '  true' > "$file"
  build_locked_expect "two trailing backslashes preserve a locked physical line" 0 "" "$file"

  file="$case_dir/three-locked.sh"
  printf '%s\n' "cargo build \\\\\\" '  --locked' > "$file"
  build_locked_expect "three trailing backslashes join a locked continuation" 0 "" "$file"

  file="$case_dir/three-unlocked.sh"
  printf '%s\n' "cargo build \\\\\\" '  --release' > "$file"
  build_locked_expect "three trailing backslashes join an unlocked continuation" 1 "MISSING --locked:" "$file"

  file="$case_dir/crlf-locked.sh"
  printf '%s\r\n' "cargo build \\" '  --locked' > "$file"
  build_locked_expect "CRLF continuation accepts --locked" 0 "" "$file"

  file="$case_dir/crlf-unlocked.sh"
  printf '%s\r\n' "cargo build \\" '  --release' > "$file"
  build_locked_expect "CRLF continuation rejects an unlocked build" 1 "MISSING --locked:" "$file"

  build_locked_expect "explicit missing target is rejected" 1 \
    "explicit scan target does not exist" "$case_dir/missing.sh"
  mkdir "$case_dir/not-a-file"
  build_locked_expect "explicit non-regular target is rejected" 1 \
    "explicit scan target is not a regular file" "$case_dir/not-a-file"

  synthetic_root="$case_dir/default-root"
  mkdir -p "$synthetic_root/scripts" "$synthetic_root/.github/workflows" \
    "$synthetic_root/mobile/scripts"
  cp "$audit" "$synthetic_root/scripts/check_build_locked.sh"
  for path in \
    .github/workflows/ci.yml \
    .github/workflows/platform-builds.yml \
    .github/workflows/nightly-gates.yml \
    .github/workflows/main-full.yml \
    .github/workflows/pr-fast.yml \
    build.sh \
    build_android.sh \
    mobile/scripts/build_android.sh \
    scripts/wasm_build_with_cache.sh \
    scripts/test_with_cache.sh \
    scripts/test_aot.sh
  do
    : > "$synthetic_root/$path"
  done
  out="$(cd "$synthetic_root" && bash scripts/check_build_locked.sh 2>&1)"
  code=$?
  if [ "$code" -eq 0 ]; then
    pass "check_build_locked.sh contract: missing built-in release.yml remains optional"
  else
    bad "check_build_locked.sh contract: missing optional release.yml exited $code. Output: $out"
  fi

  mkdir "$synthetic_root/.github/workflows/release.yml"
  out="$(cd "$synthetic_root" && bash scripts/check_build_locked.sh 2>&1)"
  code=$?
  if [ "$code" -ne 0 ] && printf '%s' "$out" | grep -qF -- \
    "required built-in scan target is not a regular file"
  then
    pass "check_build_locked.sh contract: non-regular built-in release.yml fails closed"
  else
    bad "check_build_locked.sh contract: release.yml directory was not rejected. Exit $code, output: $out"
  fi

  rm -rf "$case_dir"
}

# --------------------------------------------------------------------------
# NO_SELFTEST_REASONS — audits without a dedicated negative self-test, each with
# a written reason (Issue #9388 acceptance). Format: `basename<TAB>reason`.
# The registration-only/full completeness check requires every audit to be here
# OR covered above.
# --------------------------------------------------------------------------
# shellcheck disable=SC2016 # Literal docs-only reasons; no expansion intended.
NO_SELFTEST_REASONS='
check_numeric_matrix_reduced.sh	requires a built release sjulia binary + upstream julia oracle — no injectable violation in a source-only sandbox
check_metaprogramming_roundtrip.sh	requires a built release sjulia binary + upstream julia — no injectable violation in a source-only sandbox
check_cold_cached_nextest.sh	requires a full cargo build + nextest run — no injectable violation in a source-only sandbox
check_cache_sensitive_fixture_lane.sh	requires a full cargo build + three nextest runs across cache modes — no injectable violation in a source-only sandbox
check_fixture_parity_sweep.sh	requires a built release sjulia binary + upstream julia + the tests/fixtures tree (excluded from the sandbox) — no injectable violation in a source-only sandbox
check_vendored_drift.sh	requires network access (queries crates.io) — no injectable local violation
check_upstream_mirror_drift.sh	report-only audit (always exits 0; no failure path to trigger)
check_ffi_header_compiles.sh	requires a C/C++ compiler toolchain (compiles subset_vm.h) — not a source-only scan
check_fixture_test_names.sh	needs the tests/fixtures manifest set (fixtures are excluded from the sandbox)
check_fixture_chunk_size.sh	scans subset_julia_vm/build.rs + the tests/fixtures manifests, which are excluded from the source-only sandbox (3k+ files)
check_ios_sample_catalog.sh	needs the iOS Samples/ catalog + README (outside the source sandbox)
check_plotly_bundle.sh	needs the shipped host plotly.min.js bundles (iOS/web/mobile assets)
check_parser_corpus_allowlist.sh	needs the parser corpus sweep TSV + julia/ submodule (skips gracefully otherwise)
check_upstream_test_sweep_allowlist.sh	needs the upstream julia/test sweep TSV (skips gracefully otherwise)
check_docs_vm_refs.sh	scans CLAUDE.md + the docs/vm markdown set (agent-instruction/doc assets outside the source sandbox), not Rust/Julia source
check_ffi_abi_version.sh	needs the generated FFI header + committed signature-hash baseline — not a source-only scan
check_ffi_catch_unwind.sh	its only failure diagnostic is an aggregate `ffi_missing_catch_unwind=N` count (per-export names go to a tsv, not stdout), and it is red on main (#8740) — no injection-specific reason string distinguishes an injected export from the drifted baseline
check_codegraph_guidance_single_source.sh	operates on root agent-instruction files (AGENTS.md / CLAUDE.md symlink / .claude,.gemini,.github targets) and their symlink topology, which are not copied into the source-only sandbox
check_sample_body_consistency.sh	compares iOS Resources/Samples + Swift Models + mobile/assets sample bodies (non-source host assets outside the source sandbox)
check_fixture_manifests.sh	scans the tests/fixtures manifest.toml set, which is excluded from the source-only sandbox (3k+ files); regression-covered by fixture harness tests (Issue #9378)
check_unregistered_fixtures.sh	scans the tests/fixtures .jl tree, which is excluded from this generic source sandbox (3k+ files); independently mutation-tested in a temporary fixture tree by fixture_coverage_contract_selftest.sh (Issue #11041)
audit_health_report.sh	aggregate report runner over constituent check_*.sh/audit_*.sh scripts; no independent source invariant beyond the audits covered here
audit_native_boundary_ccall.sh	requires an upstream Julia checkout/submodule plus the generated ccall ledger; no injectable violation in the source-only sandbox
'

# reason_for <basename> — prints the NO_SELFTEST reason, or empty if none.
reason_for() {
  printf '%s\n' "$NO_SELFTEST_REASONS" | awk -F'\t' -v want="$1" '$1 == want { print $2; exit }'
}

# --------------------------------------------------------------------------
# completeness_check (Issue #9388): every scripts/check_*.sh + scripts/audit_*.sh
# (except this framework) must be covered by a run_selftest OR annotated in
# NO_SELFTEST_REASONS. The --registration-only mode executes the same top-level
# run_selftest calls but skips their sandboxes, making this check cheap enough
# for every guarded merge (Issue #11065).
# --------------------------------------------------------------------------
completeness_check() {
  local f base reason covered=0 annotated=0 unaccounted=0
  local self="check_audit_negative_selftest.sh"
  for f in "$REPO_ROOT"/scripts/check_*.sh "$REPO_ROOT"/scripts/audit_*.sh; do
    [ -f "$f" ] || continue
    base="$(basename "$f")"
    [ "$base" = "$self" ] && continue
    case "$COVERED_LIST" in
      *" $base "*) covered=$((covered + 1)); continue ;;
    esac
    reason="$(reason_for "$base")"
    if [ -n "$reason" ]; then
      annotated=$((annotated + 1))
    else
      bad "completeness: $base has neither a negative self-test nor a NO_SELFTEST_REASONS entry (Issue #9388). Add a run_selftest or annotate it with a reason."
      unaccounted=$((unaccounted + 1))
    fi
  done
  local b
  while IFS=$'\t' read -r b _; do
    [ -n "$b" ] || continue
    case "$b" in \#*) continue ;; esac
    case "$COVERED_LIST" in
      *" $b "*) log "      NOTE: NO_SELFTEST_REASONS lists '$b' but it is also covered — remove the stale annotation." ;;
    esac
    [ -f "$REPO_ROOT/scripts/$b" ] || log "      NOTE: NO_SELFTEST_REASONS lists '$b' but scripts/$b does not exist — remove the stale annotation."
  done <<EOF
$NO_SELFTEST_REASONS
EOF
  if [ "$unaccounted" -eq 0 ]; then
    pass "completeness: all audits accounted for ($covered covered, $annotated annotated with a reason)"
  fi
}

# Prevent new injectors from reintroducing the #10895/#11269 pattern: manual
# count/replace pairs coupled to incidental source spellings. Literal and regex
# edits must route through audit_selftest_edit.py, which owns exact-one failure.
anchor_policy_check() {
  local pattern='\.(replace|count)\((anchor|needle)'
  local hits inventory helper_sites inventory_rows target_rows injector target anchor_kind owner issue
  inventory="$REPO_ROOT/docs/vm/AUDIT_SELFTEST_ANCHORS.tsv"

  if ! printf '%s%s\n' 'source.replace(' 'anchor, mutation, 1)' | grep -Eq "$pattern"; then
    bad "anchor policy self-test: seeded raw replacement was not detected (Issue #11274)"
    return
  fi
  hits="$(grep -nE "$pattern" "$REPO_ROOT/scripts/check_audit_negative_selftest.sh" || true)"
  if [ -n "$hits" ]; then
    bad "anchor policy: raw anchor/needle count or replacement bypasses audit_selftest_edit.py (Issue #11274):"
    printf '%s\n' "$hits" | sed 's/^/      /'
    return
  fi
  if ! python3 scripts/audit_selftest_edit.py >/dev/null; then
    bad "anchor policy: audit_selftest_edit.py self-test failed (Issue #11274)"
    return
  fi
  if [ ! -f "$inventory" ]; then
    bad "anchor policy: missing $inventory (Issue #11274)"
    return
  fi
  if [ "$(head -1 "$inventory")" != $'injector\ttarget\tanchor_kind\tsemantic_owner\tissue' ]; then
    bad "anchor policy: malformed AUDIT_SELFTEST_ANCHORS.tsv header (Issue #11274)"
    return
  fi
  helper_sites="$(grep -Ec '^replace_(literal|regex)_exactly_once\(' "$REPO_ROOT/scripts/check_audit_negative_selftest.sh")"
  inventory_rows="$(awk -F'\t' 'NR > 1 && NF { count++ } END { print count + 0 }' "$inventory")"
  if [ "$helper_sites" -ne "$inventory_rows" ]; then
    bad "anchor policy: helper-site count $helper_sites != inventory row count $inventory_rows (Issue #11274)"
    return
  fi
  # Materialize before grep -q: with pipefail, an early grep match can SIGPIPE
  # printf once this registry exceeds the pipe buffer and report a false miss
  # (Issue #11289).
  target_rows="$(printf '%b' "$TARGET_ROWS")"
  while IFS=$'\t' read -r injector target anchor_kind owner issue; do
    [ "$injector" = "injector" ] && continue
    if [ -z "$injector" ] || [ -z "$target" ] || [ -z "$anchor_kind" ] || [ -z "$owner" ] || [ -z "$issue" ]; then
      bad "anchor policy: incomplete inventory row for '$injector' (Issue #11274)"
      return
    fi
    case "$anchor_kind" in
      literal-*|semantic-regex) ;;
      *) bad "anchor policy: invalid anchor kind '$anchor_kind' for $injector (Issue #11274)"; return ;;
    esac
    if ! grep -qF "$target"$'\t' <<< "$target_rows"; then
      bad "anchor policy: inventory target '$target' has no selectable negative control (Issue #11274)"
      return
    fi
  done < "$inventory"
  pass "anchor policy: semantic source edits use fail-loud exact-one helpers (Issue #11274)"
}

# --------------------------------------------------------------------------
# Silent-exit lint (Issue #9129 A / F5): every audit script must emit a
# human-readable diagnostic on its failure path. A script that can `exit 1`
# with no output leaves nothing in the log to say which audit broke.
# --------------------------------------------------------------------------
silent_exit_lint() {
  local diag_re='(FAIL|ERROR|error:|not found|missing|mismatch|baseline|exceed|stale|violat|must |SystemExit|sys\.stderr|file=sys\.stderr|errors\.append|>&2|raise )'
  local f base count=0 lint_fail=0
  for f in "$REPO_ROOT"/scripts/check_*.sh "$REPO_ROOT"/scripts/audit_*.sh; do
    [ -f "$f" ] || continue
    count=$((count + 1))
    base="$(basename "$f")"
    if ! grep -Eiq "$diag_re" "$f"; then
      bad "silent-exit lint: $base has no failure-diagnostic emit (no FAIL/ERROR message, >&2 write, or embedded-python error). A silent exit hides which audit broke (Issue #9129 F5 / PR #9095)."
      lint_fail=1
    fi
  done
  if [ "$lint_fail" -eq 0 ]; then
    pass "silent-exit lint: all $count audit scripts emit a failure diagnostic"
  fi
}

# check_orphaned_rs_files.sh violation (Issue #10739): drop a new .rs file
# under a crate's src/ tree that no mod/#[path]/include! ever reaches — the
# exact shape of the `subset_julia_vm/src/ir/core.rs` orphan the audit exists
# to catch (present on disk, syntactically valid, never fed to rustc).
inject_orphaned_rs_files() {
  cat > "$1/subset_julia_vm/src/selftest10739orphan.rs" <<'EOF'
// audit-selftest selftest10739orphan — deliberately never mod-declared.
pub fn selftest10739orphan_never_called() -> i32 {
    0
}
EOF
}

# check_test_binary_budget.sh violation: add a tests/*.rs binary that is NOT in
# the allowlist (the "one bug fix ≒ one new per-issue test binary" growth path).
inject_test_binary_budget() {
  mkdir -p "$1/subset_julia_vm/tests"
  printf '%s\n' '// audit-selftest SelftestUnlistedBinary9671' \
    > "$1/subset_julia_vm/tests/selftest_unlisted_9671_tests.rs"
}

# check_fixture_categories.sh violation: add a fixtures/ category directory that
# is NOT in the canonical allowlist (the near-synonym drift path, e.g. `arrays`).
inject_fixture_category() {
  mkdir -p "$1/subset_julia_vm/tests/fixtures/arrays_selftest9671"
  printf '%s\n' 'SelftestUnlistedCategory9671' \
    > "$1/subset_julia_vm/tests/fixtures/arrays_selftest9671/marker.txt"
}

# --registration-only completeness violation (Issue #11065): add a
# syntactically valid audit script with a failure diagnostic, but deliberately
# omit both a run_selftest registration and a NO_SELFTEST_REASONS annotation.
inject_unregistered_audit() {
  {
    printf '%s\n' '#!/usr/bin/env bash'
    printf '%s\n' '# audit-selftest SelftestUnregisteredAudit11065'
    printf '%s\n' 'echo "FAIL: selftest unregistered audit" >&2'
    printf '%s\n' 'exit 1'
  } > "$1/scripts/check_selftest_unregistered_example.sh"
  chmod +x "$1/scripts/check_selftest_unregistered_example.sh"
}

# archive_status_done.sh violation (Issue #11263): grow one live status file
# beyond a deliberately small self-test budget. The clean archived files fit
# that budget; the injected marker proves the check's failure is caused by the
# new live-file growth rather than an unrelated audit error.
inject_status_archive_budget_overflow() {
  current_lines=$(wc -l < "$1/docs/vm/STATUS.md")
  overflow_lines=$((3001 - current_lines))
  if [ "$overflow_lines" -lt 1 ]; then
    overflow_lines=1
  fi
  {
    printf '%s\n' '## 最新対応 (2099-12-31)'
    printf '%s\n' '<!-- SelftestStatusArchiveBudget11263 -->'
    i=0
    while [ "$i" -lt "$overflow_lines" ]; do
      printf 'selftest archive budget overflow line %s\n' "$i"
      i=$((i + 1))
    done
  } >> "$1/docs/vm/STATUS.md"
}

inject_done_archive_budget_overflow() {
  current_lines=$(wc -l < "$1/docs/vm/DONE.md")
  overflow_lines=$((3001 - current_lines))
  if [ "$overflow_lines" -lt 1 ]; then
    overflow_lines=1
  fi
  {
    printf '%s\n' '## 最新対応 (2099-12-31)'
    printf '%s\n' '<!-- SelftestDoneArchiveBudget11263 -->'
    i=0
    while [ "$i" -lt "$overflow_lines" ]; do
      printf 'selftest DONE archive budget overflow line %s\n' "$i"
      i=$((i + 1))
    done
  } >> "$1/docs/vm/DONE.md"
}

# check_source_only_audit_sync.sh violation (Issue #11065): remove the actual
# default runner command while leaving all explanatory comments intact. A
# source-text grep for the runner name would false-green this mutation; the
# audit must inspect premerge_gate.sh --list-gates instead.
inject_premerge_runner_command_removed() {
  injector_python "$1" "scripts/premerge_gate.sh" <<'PY'
import pathlib
import sys
from audit_selftest_edit import replace_literal_exactly_once

path = pathlib.Path(sys.argv[1])
needle = '    GATE_CMDS+=("bash scripts/run_source_only_audits.sh")\n'
replacement = '    : # audit-selftest SelftestPremergeRunnerRemoved11065\n'
replace_literal_exactly_once(path, needle, replacement, label="source-audit runner command")
PY
}

# check_test_aot_vm_aot_lane.sh violation 1 (Issue #10815): remove the actual
# `metamorphic_equivalence.sh ... --lane vm_aot` invocation from the mandatory
# `scripts/test_aot.sh` gate while leaving the surrounding echo/comment text
# intact — the "gate exists on paper, nothing local runs it" failure mode
# #10815 found one layer up from #10870/#10912.
inject_test_aot_vm_aot_lane_missing() {
  injector_python "$1" "scripts/test_aot.sh" <<'PY'
import pathlib
import sys
from audit_selftest_edit import replace_literal_exactly_once

path = pathlib.Path(sys.argv[1])
needle = '  timeout 1800 bash "$ROOT/scripts/metamorphic_equivalence.sh" --lane vm_aot\n'
replacement = '  : # SELFTEST10815-REMOVED-VM-AOT-LANE\n'
replace_literal_exactly_once(path, needle, replacement, label="vm_aot lane invocation")
PY
}

# check_test_aot_vm_aot_lane.sh violation 3 (Issue #11598): restore a fixed
# repository-local juliars path while leaving CARGO_TARGET_DIR and the explicit
# override syntax present. The executable contract must reject the producer /
# consumer split rather than accepting token presence alone.
inject_test_aot_fixed_binary_path() {
  injector_python "$1" "scripts/test_aot.sh" <<'PY'
import pathlib
import sys
from audit_selftest_edit import replace_literal_exactly_once

path = pathlib.Path(sys.argv[1])
needle = 'JULIARS_BIN="${JULIARS_BIN:-$cargo_target_dir/release/juliars}"\n'
replacement = 'JULIARS_BIN="${JULIARS_BIN:-$ROOT/target/release/juliars}" # SELFTEST11598-FIXED-TARGET\n'
replace_literal_exactly_once(path, needle, replacement, label="Cargo target juliars default")
PY
}

# check_test_aot_vm_aot_lane.sh violation 4 (Issue #11598): keep the expected
# target-derived assignment, then override it later in a direct helper using a
# braced fixed path. This proves the audit enforces one authoritative assignment
# rather than matching a required token and stopping.
inject_aot_vm_fixed_binary_reassignment() {
  injector_python "$1" "scripts/aot_vm_differential.sh" <<'PY'
import pathlib
import sys
from audit_selftest_edit import replace_literal_exactly_once

path = pathlib.Path(sys.argv[1])
needle = 'export JULIARS_BIN SJULIA_BIN\n'
replacement = (
    needle
    + 'JULIARS_BIN="${ROOT}/target/release/juliars" # SELFTEST11598-DIRECT-REASSIGN\n'
)
replace_literal_exactly_once(path, needle, replacement, label="direct helper binary export")
PY
}

# check_test_aot_vm_aot_lane.sh violation 5 (Issue #11598): keep both binary
# defaults intact, then redirect Cargo itself back to the repository-local
# target after the paths were derived. The executable probe must prove producer
# and consumer still share the same target directory at every Cargo invocation.
inject_test_aot_late_target_reset() {
  injector_python "$1" "scripts/test_aot.sh" <<'PY'
import pathlib
import sys
from audit_selftest_edit import replace_literal_exactly_once

path = pathlib.Path(sys.argv[1])
needle = 'export SJULIA_BIN JULIARS_BIN\n'
replacement = (
    needle
    + 'export CARGO_TARGET_DIR="$ROOT/target" # SELFTEST11598-LATE-TARGET-RESET\n'
)
replace_literal_exactly_once(path, needle, replacement, label="AoT binary export")
PY
}

# check_test_aot_vm_aot_lane.sh violation 6 (Issue #11598): leave the
# authoritative JULIARS_BIN assignment present but bypass it at the executable
# use site with an unprefixed repository-relative release path.
inject_aot_vm_bare_fixed_binary_invocation() {
  injector_python "$1" "scripts/aot_vm_differential.sh" <<'PY'
import pathlib
import sys
from audit_selftest_edit import replace_literal_exactly_once

path = pathlib.Path(sys.argv[1])
needle = '    if ! timeout 1800 "$JULIARS_BIN" "$fixture" -o "$generated_rs" --emit-binary "$aot_bin" >"$tmp_dir/juliars.out" 2>&1; then\n'
replacement = '    if ! timeout 1800 target/release/juliars "$fixture" -o "$generated_rs" --emit-binary "$aot_bin" >"$tmp_dir/juliars.out" 2>&1; then # SELFTEST11598-BARE-FIXED-INVOKE\n'
replace_literal_exactly_once(path, needle, replacement, label="direct helper juliars invocation")
PY
}

# check_test_aot_vm_aot_lane.sh violation 7 (Issue #11598): override the Cargo
# target only for one producer command. Observing timeout's outer environment is
# insufficient; the audit must inspect the environment received by Cargo.
inject_test_aot_command_local_target_reset() {
  injector_python "$1" "scripts/test_aot.sh" <<'PY'
import pathlib
import sys
from audit_selftest_edit import replace_literal_exactly_once

path = pathlib.Path(sys.argv[1])
needle = 'timeout 1800 cargo build --locked --release -p subset_julia_vm --features aot --bin juliars\n'
replacement = 'timeout 1800 env CARGO_TARGET_DIR="$ROOT/target" cargo build --locked --release -p subset_julia_vm --features aot --bin juliars # SELFTEST11598-COMMAND-TARGET-RESET\n'
replace_literal_exactly_once(path, needle, replacement, label="AoT juliars Cargo build")
PY
}

# check_test_aot_vm_aot_lane.sh violation 8 (Issue #11598): use the derived
# target path directly at an executable site. This still follows Cargo's target
# but silently disables the documented JULIARS_BIN override precedence.
inject_aot_vm_target_dir_invocation() {
  injector_python "$1" "scripts/aot_vm_differential.sh" <<'PY'
import pathlib
import sys
from audit_selftest_edit import replace_literal_exactly_once

path = pathlib.Path(sys.argv[1])
needle = '    if ! timeout 1800 "$JULIARS_BIN" "$fixture" -o "$generated_rs" --emit-binary "$aot_bin" >"$tmp_dir/juliars.out" 2>&1; then\n'
replacement = '    if ! timeout 1800 "$cargo_target_dir/release/juliars" "$fixture" -o "$generated_rs" --emit-binary "$aot_bin" >"$tmp_dir/juliars.out" 2>&1; then # SELFTEST11598-TARGET-DIR-INVOKE\n'
replace_literal_exactly_once(path, needle, replacement, label="direct helper juliars invocation")
PY
}

# check_test_aot_vm_aot_lane.sh violation 9 (Issue #11598): replace the real
# producer with a shell-hidden Cargo invocation and add a harmless direct Cargo
# command so a probe that checks only a minimum row count still passes.
inject_test_aot_shell_hidden_target_reset() {
  injector_python "$1" "scripts/test_aot.sh" <<'PY'
import pathlib
import sys
from audit_selftest_edit import replace_literal_exactly_once

path = pathlib.Path(sys.argv[1])
needle = 'timeout 1800 cargo build --locked --release -p subset_julia_vm --features aot --bin juliars\n'
replacement = (
    'timeout 1800 cargo --version\n'
    + 'timeout 1800 bash -c \'CARGO_TARGET_DIR="$PWD/target" cargo build --locked --release -p subset_julia_vm --features aot --bin juliars\' # SELFTEST11598-SHELL-HIDDEN-TARGET\n'
)
replace_literal_exactly_once(path, needle, replacement, label="AoT juliars Cargo build")
PY
}

# check_test_aot_vm_aot_lane.sh violation 10 (Issue #11598): preserve the raw
# variable-token count with an inert use while bypassing the authoritative
# variable at the actual executable site. Exact reviewed use-site identity must
# reject this compensation trick.
inject_aot_vm_inert_variable_compensation() {
  injector_python "$1" "scripts/aot_vm_differential.sh" <<'PY'
import pathlib
import sys
from audit_selftest_edit import replace_literal_exactly_once

path = pathlib.Path(sys.argv[1])
needle = '    if ! timeout 1800 "$JULIARS_BIN" "$fixture" -o "$generated_rs" --emit-binary "$aot_bin" >"$tmp_dir/juliars.out" 2>&1; then\n'
replacement = (
    '    : "$JULIARS_BIN" # SELFTEST11598-INERT-VARIABLE-USE\n'
    + '    if ! timeout 1800 "$cargo_target_dir/release/juliars" "$fixture" -o "$generated_rs" --emit-binary "$aot_bin" >"$tmp_dir/juliars.out" 2>&1; then\n'
)
replace_literal_exactly_once(path, needle, replacement, label="direct helper juliars invocation")
PY
}

# check_test_aot_vm_aot_lane.sh violation 11 (Issue #11693): remove `scope`
# from the executable nightly fixture-parity category list while leaving the
# job and sweep command intact. The workflow contract must bind the prevention
# to the actual shell arguments, not to nearby prose.
inject_nightly_fixture_scope_removed() {
  injector_python "$1" ".github/workflows/nightly-gates.yml" <<'PY'
import pathlib
import sys
from audit_selftest_edit import replace_literal_exactly_once

path = pathlib.Path(sys.argv[1])
needle = '            ref strings macros types arithmetic dispatch closures scope io\n'
replacement = '            ref strings macros types arithmetic dispatch closures io # SELFTEST11693-REMOVED-SCOPE\n'
replace_literal_exactly_once(path, needle, replacement, label="nightly fixture parity scope category")
PY
}

# check_aot_gate_selection.sh violation 1 (Issue #10866): remove the shared
# inference-core semantic root from the one canonical path manifest.
inject_aot_gate_shared_inference_removed() {
  injector_python "$1" ".github/aot-gate-paths.txt" <<'PY'
import pathlib
import sys
from audit_selftest_edit import replace_literal_exactly_once

path = pathlib.Path(sys.argv[1])
replace_literal_exactly_once(
    path,
    "subset_julia_vm_types/src/inference_core/**\n",
    "# SELFTEST10866-REMOVED-INFERENCE-CORE\n",
    label="shared inference-core AoT path",
)
PY
}

# check_aot_gate_selection.sh violation 2 (Issue #10866): remove a legacy AoT
# entry point from the canonical path union while shared-inference paths remain.
inject_aot_gate_legacy_entrypoint_removed() {
  injector_python "$1" ".github/aot-gate-paths.txt" <<'PY'
import pathlib
import sys
from audit_selftest_edit import replace_literal_exactly_once

path = pathlib.Path(sys.argv[1])
replace_literal_exactly_once(
    path,
    "subset_julia_vm/src/bin/aot.rs\n",
    "# SELFTEST10866-REMOVED-LEGACY-ENTRYPOINT\n",
    label="legacy AoT entry-point path",
)
PY
}

# check_aot_gate_selection.sh violation 3 (Issue #10866): let ci.yml drift
# away from the shared selector while pr-fast.yml remains connected.
inject_aot_gate_ci_delegation_removed() {
  injector_python "$1" ".github/workflows/ci.yml" <<'PY'
import pathlib
import sys
from audit_selftest_edit import replace_literal_exactly_once

path = pathlib.Path(sys.argv[1])
replace_literal_exactly_once(
    path,
    '          python3 scripts/select_aot_gate.py --changed-files changed-files.txt --github-output "$GITHUB_OUTPUT"\n',
    '          echo "aot=false" >> "$GITHUB_OUTPUT" # SELFTEST10866-CI-DISCONNECTED\n',
    label="ci AoT selector delegation",
)
PY
}

# check_aot_gate_selection.sh violation 4 (Issue #10866): keep selector
# delegation and output projection intact but disconnect the pr-fast consumer.
inject_aot_gate_pr_consumer_disconnected() {
  injector_python "$1" ".github/workflows/pr-fast.yml" <<'PY'
import pathlib
import sys
from audit_selftest_edit import replace_literal_exactly_once

path = pathlib.Path(sys.argv[1])
replace_literal_exactly_once(
    path,
    "    if: needs.changes.outputs.aot == 'true'\n",
    "    if: false # SELFTEST10866-PR-CONSUMER-DISCONNECTED\n",
    label="pr-fast AoT consumer condition",
)
PY
}

# check_rust_toolchain_contract.sh violation (Issue #11253): silently route the
# mandatory AoT gate through the default lane while leaving the registry intact.
inject_test_aot_clippy_lane_weakened() {
  injector_python "$1" "scripts/test_aot.sh" <<'PY'
import pathlib
import sys
from audit_selftest_edit import replace_literal_exactly_once

path = pathlib.Path(sys.argv[1])
needle = '  timeout 1800 bash "$ROOT/scripts/run_clippy_lanes.sh" aot\n'
replacement = '  timeout 1800 bash "$ROOT/scripts/run_clippy_lanes.sh" default # SELFTEST11253\n'
replace_literal_exactly_once(path, needle, replacement, label="AoT Clippy owner")
PY
}

# check_rust_toolchain_contract.sh violation (Issue #11253): add a workspace
# package that does not inherit the workspace MSRV. The audit must derive the
# package list from Cargo.toml rather than relying on a fixed manifest list.
inject_workspace_member_without_msrv() {
  python3 - "$1/Cargo.toml" <<'PY'
import pathlib
import sys

path = pathlib.Path(sys.argv[1])
src = path.read_text(encoding="utf-8")
lines = src.splitlines(keepends=True)
anchors = [i for i, line in enumerate(lines) if line == "members = [\n"]
assert len(anchors) == 1, f"workspace members anchors: {len(anchors)}"
lines.insert(anchors[0] + 1, '    "selftest_missing_msrv_11253",\n')
path.write_text("".join(lines), encoding="utf-8")
PY
  mkdir -p "$1/selftest_missing_msrv_11253"
  cat > "$1/selftest_missing_msrv_11253/Cargo.toml" <<'TOML'
[package]
name = "selftest_missing_msrv_11253"
version = "0.0.0"
edition = "2021"
# SELFTEST11253-MISSING-MSRV
TOML
}

# check_rust_toolchain_contract.sh violation (Issue #11253): remove the CI lint
# step's stable override while leaving the stable install action intact. The
# checked-in rust-toolchain.toml would otherwise silently select Rust 1.95.0.
inject_ci_lint_job_not_current_stable() {
  injector_python "$1" ".github/workflows/ci.yml" <<'PY'
import pathlib
import sys
from audit_selftest_edit import replace_literal_exactly_once

path = pathlib.Path(sys.argv[1])
needle = "RUSTUP_TOOLCHAIN: stable"
replace_literal_exactly_once(
    path,
    needle,
    "RUSTUP_TOOLCHAIN: 1.95.0 # SELFTEST11253-NOT-CURRENT-STABLE",
    label="current-stable override",
)
PY
}

# check_test_aot_vm_aot_lane.sh violation 2 (Issue #10815): shrink the vm_aot
# differential corpus back to only the 3 original acceptance kernels, undoing
# the widened coverage this Issue added.
inject_vm_aot_corpus_shrunk() {
  cat > "$1/tests/equivalence/vm_aot.tsv" <<'TSV'
name	fixture
coprime_pi_acceptance	subset_julia_vm/tests/fixtures/aot/coprime_pi_acceptance_aot.jl
aizawa_acceptance	subset_julia_vm/tests/fixtures/aot/aizawa_acceptance_aot.jl
mandelbrot_acceptance	subset_julia_vm/tests/fixtures/aot/mandelbrot_acceptance_aot.jl
# SELFTEST10815-SHRUNK-VM-AOT-CORPUS
TSV
}

# check_source_only_audit_sync.sh registry ownership violations (Issue #11065):
# prove the guarded sync gate rejects removal of either newly mandatory row,
# schema typos that would silently skip a row, and a CI workflow where only a
# static-lint textual mention remains after the executable step is removed.
inject_compile_vm_registry_row_removed() {
  python3 - "$1/scripts/source_only_audits.tsv" <<'PY'
import pathlib
import sys

path = pathlib.Path(sys.argv[1])
src = path.read_text(encoding="utf-8")
lines = src.splitlines()
matches = [line for line in lines if line.startswith("compile_vm_coupling\t")]
assert len(matches) == 1, f"compile_vm_coupling rows: {len(matches)}"
lines[lines.index(matches[0])] = "# audit-selftest SelftestCompileVmRegistryRemoved11065"
path.write_text("\n".join(lines) + "\n", encoding="utf-8")
PY
}

inject_fixture_categories_registry_row_removed() {
  python3 - "$1/scripts/source_only_audits.tsv" <<'PY'
import pathlib
import sys

path = pathlib.Path(sys.argv[1])
src = path.read_text(encoding="utf-8")
lines = src.splitlines()
matches = [line for line in lines if line.startswith("fixture_categories\t")]
assert len(matches) == 1, f"fixture_categories rows: {len(matches)}"
lines[lines.index(matches[0])] = "# audit-selftest SelftestFixtureRegistryRemoved11065"
path.write_text("\n".join(lines) + "\n", encoding="utf-8")
PY
}

inject_fixture_coverage_selftest_registry_row_removed() {
  python3 - "$1/scripts/source_only_audits.tsv" <<'PY'
import pathlib
import sys

path = pathlib.Path(sys.argv[1])
src = path.read_text(encoding="utf-8")
lines = src.splitlines()
matches = [line for line in lines if line.startswith("fixture_coverage_contract_selftest\t")]
assert len(matches) == 1, f"fixture coverage self-test rows: {len(matches)}"
lines[lines.index(matches[0])] = "# audit-selftest SelftestFixtureCoverageRegistryRemoved11041"
path.write_text("\n".join(lines) + "\n", encoding="utf-8")
PY
}

inject_definition_order_registry_row_removed() {
  python3 - "$1/scripts/source_only_audits.tsv" <<'PY'
import pathlib
import sys

path = pathlib.Path(sys.argv[1])
src = path.read_text(encoding="utf-8")
lines = src.splitlines()
matches = [line for line in lines if line.startswith("definition_order_merges\t")]
assert len(matches) == 1, f"definition-order merge rows: {len(matches)}"
lines[lines.index(matches[0])] = "# audit-selftest SelftestDefinitionOrderRegistryRemoved11036"
path.write_text("\n".join(lines) + "\n", encoding="utf-8")
PY
}

inject_status_done_archive_registry_row_removed() {
  python3 - "$1/scripts/source_only_audits.tsv" <<'PY'
import pathlib
import sys

path = pathlib.Path(sys.argv[1])
src = path.read_text(encoding="utf-8")
lines = src.splitlines()
matches = [line for line in lines if line.startswith("status_done_archive_budget\t")]
assert len(matches) == 1, f"status/DONE archive budget rows: {len(matches)}"
lines[lines.index(matches[0])] = "# audit-selftest SelftestStatusDoneRegistryRemoved11263"
path.write_text("\n".join(lines) + "\n", encoding="utf-8")
PY
}

inject_status_done_archive_registry_row_weakened() {
  injector_python "$1" "scripts/source_only_audits.tsv" <<'PY'
import pathlib
import sys
from audit_selftest_edit import replace_literal_exactly_once

path = pathlib.Path(sys.argv[1])
needle = "status_done_archive_budget\tcheck_status_done_archive_budget.sh\ttrue\t"
replacement = "status_done_archive_budget\tcheck_status_done_archive_budget.sh\tfalse\t"
replace_literal_exactly_once(
    path, needle, replacement, label="STATUS/DONE archive budget default row"
)
with path.open("a", encoding="utf-8") as output:
    output.write("# audit-selftest SelftestStatusDoneRegistryWeakened11263\n")
PY
}

inject_base_cache_schema_registry_row_weakened() {
  injector_python "$1" "scripts/source_only_audits.tsv" <<'PY'
import pathlib
import sys
from audit_selftest_edit import replace_literal_exactly_once

path = pathlib.Path(sys.argv[1])
needle = "base_cache_schema_fingerprint\taudit_base_cache_schema_fingerprint.sh\ttrue\t"
replacement = "base_cache_schema_fingerprint\taudit_base_cache_schema_fingerprint.sh\tfalse\t"
replace_literal_exactly_once(
    path, needle, replacement, label="Base cache schema fingerprint registry row"
)
with path.open("a", encoding="utf-8") as output:
    output.write("# audit-selftest SelftestBaseCacheSchemaRegistryWeakened10688\n")
PY
}

inject_exception_payload_registry_row_weakened() {
  injector_python "$1" "scripts/source_only_audits.tsv" <<'PY'
import pathlib
import sys
from audit_selftest_edit import replace_literal_exactly_once

path = pathlib.Path(sys.argv[1])
needle = "exception_payload_carrier\taudit_exception_payload_carrier.sh\ttrue\t"
replacement = "exception_payload_carrier\taudit_exception_payload_carrier.sh\tfalse\t"
replace_literal_exactly_once(
    path, needle, replacement, label="exception payload carrier registry row"
)
with path.open("a", encoding="utf-8") as output:
    output.write("# audit-selftest SelftestExceptionPayloadRegistryWeakened11647\n")
PY
}

inject_constructor_owner_registry_row_weakened() {
  injector_python "$1" "scripts/source_only_audits.tsv" <<'PY'
import pathlib
import sys
from audit_selftest_edit import replace_literal_exactly_once

path = pathlib.Path(sys.argv[1])
needle = "constructor_owner_resolution\tcheck_constructor_owner_resolution.sh\ttrue\t"
replacement = "constructor_owner_resolution\tcheck_constructor_owner_resolution.sh\tfalse\t"
replace_literal_exactly_once(
    path, needle, replacement, label="constructor owner audit registry row"
)
with path.open("a", encoding="utf-8") as output:
    output.write("# audit-selftest SelftestConstructorOwnerRegistryWeakened11172\n")
PY
}

inject_constructor_return_registry_row_weakened() {
  injector_python "$1" "scripts/source_only_audits.tsv" <<'PY'
import pathlib
import sys
from audit_selftest_edit import replace_literal_exactly_once

path = pathlib.Path(sys.argv[1])
needle = "constructor_return_identity\tcheck_constructor_return_identity.sh\ttrue\t"
replacement = "constructor_return_identity\tcheck_constructor_return_identity.sh\tfalse\t"
replace_literal_exactly_once(
    path, needle, replacement, label="constructor return identity audit registry row"
)
with path.open("a", encoding="utf-8") as output:
    output.write("# audit-selftest SelftestConstructorReturnRegistryWeakened11436\n")
PY
}

inject_binding_provenance_registry_row_weakened() {
  injector_python "$1" "scripts/source_only_audits.tsv" <<'PY'
import pathlib
import sys
from audit_selftest_edit import replace_literal_exactly_once

path = pathlib.Path(sys.argv[1])
needle = "binding_provenance_authority\tcheck_binding_provenance_authority.sh\ttrue\ttrue\t"
replacement = "binding_provenance_authority\tcheck_binding_provenance_authority.sh\tfalse\ttrue\t"
replace_literal_exactly_once(
    path, needle, replacement, label="binding provenance audit registry row"
)
with path.open("a", encoding="utf-8") as output:
    output.write("# audit-selftest SelftestBindingProvenanceRegistryWeakened11317\n")
PY
}

inject_base_exports_registry_row_weakened() {
  injector_python "$1" "scripts/source_only_audits.tsv" <<'PY'
import pathlib
import sys
from audit_selftest_edit import replace_literal_exactly_once

path = pathlib.Path(sys.argv[1])
needle = "base_exports_subset\tcheck_base_exports_subset.sh\ttrue\ttrue\t"
replacement = "base_exports_subset\tcheck_base_exports_subset.sh\tfalse\ttrue\t"
replace_literal_exactly_once(
    path, needle, replacement, label="Base export audit registry row"
)
with path.open("a", encoding="utf-8") as output:
    output.write("# audit-selftest SelftestBaseExportRegistryWeakened11298\n")
PY
}

inject_base_exports_non_upstream() {
  injector_python "$1" "subset_julia_vm/src/julia/base/exports.jl" <<'PY'
import pathlib
import sys
from audit_selftest_edit import replace_literal_exactly_once

path = pathlib.Path(sys.argv[1])
needle = "    Core,\n    Main,"
replacement = "    Core,\n    Base, # SelftestBaseExport11298\n    Main,"
replace_literal_exactly_once(path, needle, replacement, label="Base export list")
PY
}

inject_source_position_registry_row_weakened() {
  injector_python "$1" "scripts/source_only_audits.tsv" <<'PY'
import pathlib
import sys
from audit_selftest_edit import replace_literal_exactly_once

path = pathlib.Path(sys.argv[1])
needle = "source_position_chronology\tcheck_source_position_chronology.sh\ttrue\ttrue\t"
replacement = "source_position_chronology\tcheck_source_position_chronology.sh\tfalse\ttrue\t"
replace_literal_exactly_once(
    path, needle, replacement, label="source-position chronology audit registry row"
)
with path.open("a", encoding="utf-8") as output:
    output.write("# audit-selftest SelftestSourcePositionRegistryWeakened11100\n")
PY
}

inject_source_position_api_raw_usize() {
  injector_python "$1" "subset_julia_vm_lowering/src/lowering/type_alias.rs" <<'PY'
import pathlib
import sys
from audit_selftest_edit import replace_literal_exactly_once

path = pathlib.Path(sys.argv[1])
needle = "pub fn expand_for_signature(name: &str, use_position: SourcePosition) -> String {"
replacement = "pub fn expand_for_signature(name: &str, use_start: usize) -> String { // SelftestSourcePositionApi11100"
replace_literal_exactly_once(path, needle, replacement, label="expand_for_signature typed position API")
PY
}

inject_source_position_raw_offset_compare() {
  injector_python "$1" "subset_julia_vm_lowering/src/lowering/type_alias.rs" <<'PY'
import pathlib
import sys
from audit_selftest_edit import replace_literal_exactly_once

path = pathlib.Path(sys.argv[1])
needle = "fn entry_is_available(entry: &AliasEntry, mode: ResolutionMode) -> bool {"
replacement = """fn selftest_raw_source_order_11100(definition_start: usize, use_start: usize) -> bool {
    definition_start <= use_start // SelftestRawSourceOrder11100
}

fn entry_is_available(entry: &AliasEntry, mode: ResolutionMode) -> bool {"""
replace_literal_exactly_once(path, needle, replacement, label="entry availability authority")
PY
}

inject_source_position_raw_offset_cmp() {
  injector_python "$1" "subset_julia_vm_lowering/src/lowering/type_alias.rs" <<'PY'
import pathlib
import sys
from audit_selftest_edit import replace_literal_exactly_once

path = pathlib.Path(sys.argv[1])
needle = "fn entry_is_available(entry: &AliasEntry, mode: ResolutionMode) -> bool {"
replacement = """fn selftest_raw_source_order_cmp_11100(definition_start: usize, use_start: usize) -> bool {
    definition_start.cmp(&use_start).is_le() // SelftestRawSourceOrderCmp11100
}

fn entry_is_available(entry: &AliasEntry, mode: ResolutionMode) -> bool {"""
replace_literal_exactly_once(path, needle, replacement, label="entry availability cmp authority")
PY
}

inject_registry_boolean_typo() {
  injector_python "$1" "scripts/source_only_audits.tsv" <<'PY'
import pathlib
import sys
from audit_selftest_edit import replace_literal_exactly_once

path = pathlib.Path(sys.argv[1])
needle = "fixture_categories\tcheck_fixture_categories.sh\ttrue\ttrue\t"
replacement = (
    "fixture_categories\tcheck_fixture_categories.sh\tture\ttrue\t"
)
replace_literal_exactly_once(path, needle, replacement, label="fixture category registry row")
with path.open("a", encoding="utf-8") as output:
    output.write("# audit-selftest SelftestRegistryBooleanTypo11065\n")
PY
}

inject_ci_executable_step_removed() {
  injector_python "$1" ".github/workflows/ci.yml" <<'PY'
import pathlib
import sys
from audit_selftest_edit import replace_literal_exactly_once

path = pathlib.Path(sys.argv[1])
needle = "        run: bash scripts/check_fixture_categories.sh\n"
replacement = "        # audit-selftest SelftestCiExecutableStepRemoved11065\n"
replace_literal_exactly_once(path, needle, replacement, label="CI fixture-category step")
PY
}

# check_julia_display_write_text_paths.sh violation: reintroduce the bug class
# where a display helper routes an arbitrary argument through binary write.
inject_julia_display_write_text_paths() {
  cat >> "$1/subset_julia_vm/src/julia/base/io.jl" <<'JL'

# audit-selftest SelftestDisplayWriteTextPath10008
function selftest_display_write_text_path_10008(io, arg)
    write(io, arg)
end
JL
}

# check_constructor_identity_authority.sh violation: reintroduce the split
# per-signature boolean that can disagree with MethodTable's serialized family
# map after deduplication or cache replay (Issue #11043).
inject_constructor_identity_side_boolean() {
  injector_python "$1" "subset_julia_vm_bytecode/src/method_table.rs" <<'PY'
import pathlib
import sys
from audit_selftest_edit import replace_literal_exactly_once

path = pathlib.Path(sys.argv[1])
needle = "pub struct MethodSig {\n"
replacement = (
    needle
    + "    // audit-selftest SelftestConstructorIdentitySideBoolean11043\n"
    + "    pub is_inner_constructor: bool,\n"
)
replace_literal_exactly_once(path, needle, replacement, label="MethodSig declaration")
PY
}

inject_constructor_identity_disabled_query() {
  injector_python "$1" "subset_julia_vm_bytecode/src/method_table.rs" <<'PY'
import pathlib
import sys
from audit_selftest_edit import replace_literal_exactly_once

path = pathlib.Path(sys.argv[1])
needle = "self.constructor_self_families.contains_key(&global_index)"
replacement = (
    "false && self.constructor_self_families.contains_key(&global_index) "
    "/* SelftestConstructorIdentityDisabledQuery11043 */"
)
replace_literal_exactly_once(path, needle, replacement, label="constructor authority query")
PY
}

inject_constructor_identity_disabled_selector() {
  injector_python "$1" "subset_julia_vm_compile/src/compile/expr/call/constructors.rs" <<'PY'
import pathlib
import sys
from audit_selftest_edit import replace_literal_exactly_once

path = pathlib.Path(sys.argv[1])
needle = "if table.is_inner_constructor(method.global_index)\n                    ||"
replacement = (
    "if table.is_inner_constructor(method.global_index) && false "
    "/* SelftestConstructorIdentityDisabledSelector11043 */\n                    ||"
)
replace_literal_exactly_once(path, needle, replacement, label="constructor selector guard")
PY
}

# check_constructor_return_identity.sh violation: restore the retired
# same-base first-match sharpening that made constructor results depend on
# registration/HashMap order (Issue #11436, prevention for bug #11434).
inject_constructor_return_family_first_match() {
  injector_python "$1" "subset_julia_vm_compile/src/compile/context.rs" <<'PY'
import pathlib
import sys
from audit_selftest_edit import replace_literal_exactly_once

path = pathlib.Path(sys.argv[1])
needle = '''    pub fn get_struct_type_id(&self, name: &str) -> Option<usize> {
        self.struct_table
            .resolve(name)
            .map(|(_, info)| info.type_id)
    }
'''
replacement = '''    pub fn get_struct_type_id(&self, name: &str) -> Option<usize> {
        // audit-selftest SelftestConstructorReturnFamilyFirstMatch11436
        let prefix = format!("{}{{", name);
        self.struct_table
            .iter()
            .find(|(candidate, _)| candidate.starts_with(&prefix))
            .map(|(_, info)| info.type_id)
    }
'''
replace_literal_exactly_once(path, needle, replacement, label="get_struct_type_id exact lookup")
PY
}

# check_constructor_return_identity.sh violation: restore the CoreCompiler
# same-family first-sibling conversion removed by Issue #11436.
inject_constructor_return_core_family_first_match() {
  injector_python "$1" "subset_julia_vm_compile/src/compile/core_compiler.rs" <<'PY'
import pathlib
import sys
from audit_selftest_edit import replace_literal_exactly_once

path = pathlib.Path(sys.argv[1])
needle = '''                if let Some(info) = self.resolve_struct_info_scoped(name) {
                    ValueType::Struct(info.type_id)
                } else {
                    // A missed complete name cannot be replaced with the bare
                    // family or an arbitrary registered sibling. Runtime
                    // specialization resolves the concrete value (Issue #11436).
                    ValueType::Any
                }
'''
replacement = '''                if let Some(info) = self.resolve_struct_info_scoped(name) {
                    ValueType::Struct(info.type_id)
                } else {
                    // audit-selftest SelftestConstructorReturnCoreFamilyFirstMatch11436
                    let base = name.split('{').next().unwrap_or(name);
                    for (registered_name, info) in &self.shared_ctx.struct_table {
                        if registered_name.starts_with(base) {
                            return ValueType::Struct(info.type_id);
                        }
                    }
                    ValueType::Any
                }
'''
replace_literal_exactly_once(
    path, needle, replacement, label="CoreCompiler exact-or-Any conversion"
)
PY
}

# check_constructor_return_identity.sh violation: make an explicit complete
# typed-array element such as Rational{Int64} probe only its bare family.
inject_constructor_return_typed_array_family_lookup() {
  injector_python "$1" "subset_julia_vm_compile/src/compile/expr/builtin_array.rs" <<'PY'
import pathlib
import sys
from audit_selftest_edit import replace_literal_exactly_once

path = pathlib.Path(sys.argv[1])
needle = "if let Some(info) = self.resolve_struct_info_scoped(name) {"
replacement = (
    "if let Some(info) = self.resolve_struct_info_scoped(base_name) { "
    "// audit-selftest SelftestConstructorReturnTypedArrayFamilyLookup11436"
)
replace_literal_exactly_once(
    path, needle, replacement, label="typed-array exact element lookup"
)
PY
}

inject_constructor_return_instantiated_typevar() {
  injector_python "$1" "subset_julia_vm_compile/src/compile/expr/infer/expr_tfuncs.rs" <<'PY'
import pathlib
import sys
from audit_selftest_edit import replace_literal_exactly_once

path = pathlib.Path(sys.argv[1])
needle = ".all(|arg| constructor_type_expr_is_complete(arg, is_active_type_param))"
replacement = (
    ".all(|_| true) "
    "// audit-selftest SelftestConstructorReturnInstantiatedTypevar11436"
)
replace_literal_exactly_once(
    path, needle, replacement, label="instantiated constructor completeness guard"
)
PY
}

inject_constructor_return_unresolved_owner_inference() {
  injector_python "$1" "subset_julia_vm_compile/src/compile/expr/infer/mod.rs" <<'PY'
import pathlib
import sys
from audit_selftest_edit import replace_literal_exactly_once

path = pathlib.Path(sys.argv[1])
needle = '''                                expr_tfuncs::infer_value_parametric_struct_ctor(
                                    &resolved_name,
                                    &mut inst,
                                    &arg_types,
                                )
'''
replacement = '''                                expr_tfuncs::infer_value_parametric_struct_ctor(
                                    function,
                                    &mut inst,
                                    &arg_types,
                                ) // audit-selftest SelftestConstructorReturnUnresolvedOwnerInference11510
'''
replace_literal_exactly_once(
    path, needle, replacement, label="resolved-owner constructor inference"
)
PY
}

inject_constructor_owner_short_fallback() {
  injector_python "$1" "subset_julia_vm_compile/src/compile/expr/call/constructors.rs" <<'PY'
import pathlib
import re
import sys
from audit_selftest_edit import replace_regex_exactly_once

path = pathlib.Path(sys.argv[1])
pattern = re.compile(
    r"(?ms)^(?P<indent>[ \t]*)pub\(super\)\s+fn\s+"
    r"try_compile_struct_table_constructor_call\s*\(.*?"
    r"^(?P=indent)\)\s*->\s*CResult<Option<ValueType>>\s*\{\n"
)

def inject(match):
    indent = match.group("indent") + "    "
    mutation = (
        f"{indent}let _selftest11172_owner_losing_fallback = "
        "short_constructor_name(function);\n"
    )
    return match.group(0) + mutation

replace_regex_exactly_once(
    path,
    pattern,
    inject,
    label="try_compile_struct_table_constructor_call owner",
)
PY
}

inject_constructor_runtime_callee_probe_removed() {
  injector_python "$1" "subset_julia_vm_compile/src/compile/expr/call/module_call.rs" <<'PY'
import pathlib
import sys
from audit_selftest_edit import replace_literal_exactly_once

path = pathlib.Path(sys.argv[1])
replace_literal_exactly_once(
    path,
    "self.runtime_nominal_binding_name(&type_name)",
    "None::<String> /* audit-selftest SelftestRuntimeCalleeProbe11713 */",
    label="runtime constructor callee probe",
)
PY
}

inject_constructor_parametric_runtime_probe_removed() {
  injector_python "$1" "subset_julia_vm_compile/src/compile/expr/call/module_call.rs" <<'PY'
import pathlib
import sys
from audit_selftest_edit import replace_literal_exactly_once

path = pathlib.Path(sys.argv[1])
replace_literal_exactly_once(
    path,
    "self.runtime_nominal_binding_name(&base_name)",
    "None::<String> /* audit-selftest SelftestParametricRuntimeProbe11713 */",
    label="parametric runtime constructor base probe",
)
PY
}

inject_constructor_dynamic_parametric_runtime_probe_removed() {
  injector_python "$1" "subset_julia_vm_compile/src/compile/expr/call/dynamic.rs" <<'PY'
import pathlib
import sys
from audit_selftest_edit import replace_literal_exactly_once

path = pathlib.Path(sys.argv[1])
replace_literal_exactly_once(
    path,
    "if let Some(runtime_binding) = self.runtime_nominal_binding_name(&qualified_base_name) {\n            // Whole-program metadata",
    "if let Some(runtime_binding) = None::<String> {\n            // audit-selftest SelftestDynamicParametricRuntimeProbe11713\n            // Whole-program metadata",
    label="dynamic parametric runtime constructor base probe",
)
PY
}

inject_constructor_runtime_probe_base_origin_guard_removed() {
  injector_python "$1" "subset_julia_vm_compile/src/compile/expr/call/dynamic.rs" <<'PY'
import pathlib
import sys
from audit_selftest_edit import replace_literal_exactly_once

path = pathlib.Path(sys.argv[1])
replace_literal_exactly_once(
    path,
    "if self.type_is_base_origin(type_name) {",
    "if false { // audit-selftest SelftestRuntimeProbeBaseOrigin11716",
    label="runtime nominal Base-origin exclusion",
)
PY
}

inject_constructor_runtime_probe_current_input_guard_removed() {
  injector_python "$1" "subset_julia_vm_compile/src/compile/expr/call/dynamic.rs" <<'PY'
import pathlib
import sys
from audit_selftest_edit import replace_literal_exactly_once

path = pathlib.Path(sys.argv[1])
replace_literal_exactly_once(
    path,
    "if !is_current_input {",
    "if false { // audit-selftest SelftestRuntimeProbeCurrentInput11716",
    label="runtime nominal current-input exclusion",
)
PY
}

inject_constructor_runtime_lexical_owner_preference_removed() {
  injector_python "$1" "subset_julia_vm_compile/src/compile/expr/call/dynamic.rs" <<'PY'
import pathlib
import sys
from audit_selftest_edit import replace_literal_exactly_once

path = pathlib.Path(sys.argv[1])
replace_literal_exactly_once(
    path,
    "let runtime_binding = lexical_qualified\n            .filter(|qualified| {",
    "let runtime_binding = lexical_qualified\n            .filter(|_| false) // audit-selftest SelftestRuntimeLexicalOwner11733\n            .filter(|qualified| {",
    label="runtime nominal lexical owner preference",
)
PY
}

inject_constructor_runtime_bare_route_removed() {
  injector_python "$1" "subset_julia_vm_compile/src/compile/expr/call/mod.rs" <<'PY'
import pathlib
import sys
from audit_selftest_edit import replace_literal_exactly_once

path = pathlib.Path(sys.argv[1])
replace_literal_exactly_once(
    path,
    "// (Issue #11733).\n        if let Some(runtime_binding) = self.runtime_nominal_binding_name(function) {",
    "// (Issue #11733).\n        if let Some(runtime_binding) = None::<String> { // audit-selftest SelftestRuntimeBareRoute11733",
    label="runtime nominal bare constructor route",
)
PY
}

inject_constructor_runtime_enum_probe_removed() {
  injector_python "$1" "subset_julia_vm_compile/src/compile/expr/call/mod.rs" <<'PY'
import pathlib
import sys
from audit_selftest_edit import replace_literal_exactly_once

path = pathlib.Path(sys.argv[1])
replace_literal_exactly_once(
    path,
    "let enum_name =\n                    if let Some(runtime_binding) = self.runtime_nominal_binding_name(function) {",
    "let enum_name =\n                    if let Some(runtime_binding) = None::<String> { // audit-selftest SelftestRuntimeEnumProbe11733",
    label="runtime enum lexical binding probe",
)
PY
}

inject_constructor_runtime_enum_member_binding_removed() {
  injector_python "$1" "subset_julia_vm_compile/src/compile/collect.rs" <<'PY'
import pathlib
import sys
from audit_selftest_edit import replace_literal_exactly_once

path = pathlib.Path(sys.argv[1])
replace_literal_exactly_once(
    path,
    "RuntimeNominalDef::Enum(enum_def)",
    "RuntimeNominalDef::Struct(_) /* audit-selftest SelftestRuntimeEnumMember11733 */",
    label="runtime enum module-member binding collection",
)
PY
}

inject_constructor_static_registry_bare_route_guard_removed() {
  injector_python "$1" "subset_julia_vm_compile/src/compile/expr/call/mod.rs" <<'PY'
import pathlib
import sys
from audit_selftest_edit import replace_literal_exactly_once

path = pathlib.Path(sys.argv[1])
replace_literal_exactly_once(
    path,
    "(!self.shared_ctx.struct_table.contains_key(&static_binding)",
    "(true /* audit-selftest SelftestStaticRegistryBareRoute11716 */",
    label="static registry exclusion from runtime bare route",
)
PY
}

inject_constructor_package_type_provenance_removed() {
  injector_python "$1" "subset_julia_vm/src/loader.rs" <<'PY'
import pathlib
import sys
from audit_selftest_edit import replace_literal_exactly_once

path = pathlib.Path(sys.argv[1])
replace_literal_exactly_once(
    path,
    "module_value.mark_as_package_origin();",
    "// audit-selftest SelftestPackageTypeProvenance11716",
    label="package type chronology provenance",
)
PY
}

inject_constructor_base_module_provenance_removed() {
  injector_python "$1" "subset_julia_vm_types/src/ir/core.rs" <<'PY'
import pathlib
import sys
from audit_selftest_edit import replace_literal_exactly_once

path = pathlib.Path(sys.argv[1])
replace_literal_exactly_once(
    path,
    "self.is_base_origin = true;",
    "// audit-selftest SelftestBaseModuleProvenance11737",
    label="Base module nominal provenance",
)
PY
}

inject_constructor_current_main_nominal_boundary_removed() {
  injector_python "$1" "subset_julia_vm_compile/src/compile/pipeline_ctx.rs" <<'PY'
import pathlib
import sys
from audit_selftest_edit import replace_literal_exactly_once

path = pathlib.Path(sys.argv[1])
replace_literal_exactly_once(
    path,
    ".position(is_base_user_main_boundary)\n            .map_or(program.main.stmts.as_slice(), |boundary| {",
    ".position(|_| false) // audit-selftest SelftestCurrentMainNominalBoundary11737\n            .map_or(program.main.stmts.as_slice(), |boundary| {",
    label="current-main runtime nominal provenance boundary",
)
PY
}

inject_constructor_inherited_module_nominal_filter_removed() {
  injector_python "$1" "subset_julia_vm_compile/src/compile/collect.rs" <<'PY'
import pathlib
import sys
from audit_selftest_edit import replace_literal_exactly_once

path = pathlib.Path(sys.argv[1])
replace_literal_exactly_once(
    path,
    "if module.is_base_origin || module.is_package_origin {",
    "if false { // audit-selftest SelftestInheritedModuleNominalFilter11737",
    label="inherited-module runtime nominal provenance filter",
)
PY
}

inject_constructor_current_input_type_provenance_removed() {
  injector_python "$1" "subset_julia_vm/src/repl/session.rs" <<'PY'
import pathlib
import sys
from audit_selftest_edit import replace_literal_exactly_once

path = pathlib.Path(sys.argv[1])
replace_literal_exactly_once(
    path,
    "&current_input_type_names,",
    "&std::collections::HashSet::new(), // audit-selftest SelftestCurrentInputTypeProvenance11716",
    label="current-input nominal provenance",
)
PY
}

inject_constructor_synthetic_span_guard_removed() {
  injector_python "$1" "subset_julia_vm_compile/src/compile/expr/call/mod.rs" <<'PY'
import pathlib
import sys
from audit_selftest_edit import replace_literal_exactly_once

path = pathlib.Path(sys.argv[1])
replace_literal_exactly_once(
    path,
    "if call_span.start != call_span.end\n                        && !type_position.is_before(call_span.definition_order, call_span.start)",
    "if !type_position.is_before(call_span.definition_order, call_span.start) // audit-selftest SelftestSyntheticSpanGuard11716",
    label="synthetic restoration span exclusion",
)
PY
}

inject_constructor_inner_parametric_runtime_probe_removed() {
  injector_python "$1" "subset_julia_vm_compile/src/compile/expr/call/dynamic.rs" <<'PY'
import pathlib
import sys
from audit_selftest_edit import replace_literal_exactly_once

path = pathlib.Path(sys.argv[1])
replace_literal_exactly_once(
    path,
    "if let Some(runtime_binding) = self.runtime_nominal_binding_name(&qualified_base_name) {\n            self.emit(Instr::ProbeRuntimeBinding(runtime_binding));",
    "if let Some(runtime_binding) = None::<String> {\n            // audit-selftest SelftestInnerParametricRuntimeProbe11713\n            self.emit(Instr::ProbeRuntimeBinding(runtime_binding));",
    label="inner parametric runtime constructor base probe",
)
PY
}

inject_constructor_splat_parametric_runtime_probe_removed() {
  injector_python "$1" "subset_julia_vm_compile/src/compile/expr/call/constructors.rs" <<'PY'
import pathlib
import sys
from audit_selftest_edit import replace_literal_exactly_once

path = pathlib.Path(sys.argv[1])
replace_literal_exactly_once(
    path,
    "self.runtime_nominal_binding_name(&resolved_base_name)",
    "None::<String> /* audit-selftest SelftestSplatParametricRuntimeProbe11713 */",
    label="splat parametric runtime constructor base probe",
)
PY
}

inject_constructor_static_callee_materialization_removed() {
  injector_python "$1" "subset_julia_vm_compile/src/compile/expr/call/module_call.rs" <<'PY'
import pathlib
import sys
from audit_selftest_edit import replace_literal_exactly_once

path = pathlib.Path(sys.argv[1])
replace_literal_exactly_once(
    path,
    "self.emit(Instr::PushDataType(type_name));",
    "self.emit(Instr::ProbeRuntimeBinding(type_name)); // audit-selftest SelftestStaticCallee11716",
    label="static constructor callee materialization",
)
PY
}

inject_constructor_static_forward_guard_removed() {
  injector_python "$1" "subset_julia_vm_compile/src/compile/expr/call/module_call.rs" <<'PY'
import pathlib
import sys
from audit_selftest_edit import replace_literal_exactly_once

path = pathlib.Path(sys.argv[1])
replace_literal_exactly_once(
    path,
    "self.emit(Instr::ThrowUndefVarError(constructor_base));",
    "self.emit(Instr::PushDataType(constructor_base)); // audit-selftest SelftestStaticForward11720",
    label="static forward constructor guard",
)
PY
}

inject_math_router_fabricates_concrete() {
  injector_python "$1" "subset_julia_vm_compile/src/compile/tfuncs/complex_ops.rs" <<'PY'
import pathlib
import sys
from audit_selftest_edit import replace_literal_exactly_once

path = pathlib.Path(sys.argv[1])
replace_literal_exactly_once(
    path,
    "pub fn tfunc_complex_contextual(_args: &[LatticeType], _ctx: &TFuncContext) -> LatticeType {\n    LatticeType::Top\n}",
    "pub fn tfunc_complex_contextual(_args: &[LatticeType], _ctx: &TFuncContext) -> LatticeType {\n    LatticeType::Concrete(ConcreteType::Struct { name: \"Complex{Float64}\".to_string(), type_id: 0 })\n}",
    label="math router fabricates concrete",
)
PY
}

inject_struct_registry_first_match_scan() {
  injector_python "$1" "subset_julia_vm_compile/src/compile/expr/call/constructors.rs" <<'PY'
import pathlib
import re
import sys
from audit_selftest_edit import replace_regex_exactly_once

path = pathlib.Path(sys.argv[1])
pattern = re.compile(
    r"(?ms)^(?P<indent>[ \t]*)pub\(super\)\s+fn\s+"
    r"try_compile_struct_table_constructor_call\s*\(.*?"
    r"^(?P=indent)\)\s*->\s*CResult<Option<ValueType>>\s*\{\n"
)

def inject(match):
    indent = match.group("indent") + "    "
    mutation = (
        f"{indent}let _selftest11436_first_match_scan = self.shared_ctx"
        ".parametric_structs.iter().find(|(name, _)| name.is_empty());\n"
    )
    return match.group(0) + mutation

replace_regex_exactly_once(
    path,
    pattern,
    inject,
    label="struct registry first-winner scan",
)
PY
}

inject_constructor_runtime_owner_guard_disabled() {
  injector_python "$1" "subset_julia_vm_vm/src/vm/exec/call_function_variable.rs" <<'PY'
import pathlib
import sys
from audit_selftest_edit import replace_literal_exactly_once

path = pathlib.Path(sys.argv[1])
needle = '''    fn constructor_type_heads_match(left: &str, right: &str) -> bool {
        if left.contains('.') || right.contains('.') {
            left == right
        } else {
            Self::type_heads_match(left, right)
        }
    }
'''
replacement = '''    fn constructor_type_heads_match(left: &str, right: &str) -> bool {
        // audit-selftest SelftestConstructorRuntimeOwnerGuard11172
        Self::type_heads_match(left, right)
    }
'''
replace_literal_exactly_once(
    path, needle, replacement, label="runtime constructor type-head owner guard"
)
PY
}

inject_constructor_base_parametric_registry_removed() {
  injector_python "$1" "subset_julia_vm_compile/src/compile/pipeline_ctx.rs" <<'PY'
import pathlib
import sys
from audit_selftest_edit import replace_literal_exactly_once

path = pathlib.Path(sys.argv[1])
needle = """                    // Top-level bundled Base structs are represented by bare
                    // IR names. Preserve their explicit `Base.T` owner as a
                    // second registry binding before a user module can replace
                    // the source-visible bare alias (Issue #11369).
                    if module_path.is_none() && stored_def.is_base_origin {
"""
replacement = """                    // SelftestConstructorBaseParametricRegistry11369
                    if stored_def.is_base_origin {
"""
replace_literal_exactly_once(
    path,
    needle,
    replacement,
    label="cached-lane Base parametric registry guard",
)
PY
}

inject_constructor_base_collection_owner_erased() {
  injector_python "$1" "subset_julia_vm_compile/src/compile/expr/call/mod.rs" <<'PY'
import pathlib
import sys
from audit_selftest_edit import replace_literal_exactly_once

path = pathlib.Path(sys.argv[1])
replace_literal_exactly_once(
    path,
    'resolve_instantiation_with_type_expr("Base.Dict", &type_args)',
    'resolve_instantiation_with_type_expr("Dict", &type_args) /* SelftestConstructorBaseCollectionOwner11369 */',
    label="explicit Base.Dict instantiation owner",
)
PY
}

inject_constructor_base_parametric_lowering_erased() {
  injector_python "$1" "subset_julia_vm_lowering/src/lowering/expr/call.rs" <<'PY'
import pathlib
import sys
from audit_selftest_edit import replace_literal_exactly_once

path = pathlib.Path(sys.argv[1])
replace_literal_exactly_once(
    path,
    'if module != "Base" || leaf.is_empty() {',
    'if module.is_empty() || leaf.is_empty() { // SelftestConstructorBaseParametricLowering11369',
    label="Base parametric call target owner guard",
)
PY
}

inject_constructor_base_nested_field_owner_erased() {
  injector_python "$1" "subset_julia_vm_compile/src/compile/context.rs" <<'PY'
import pathlib
import sys
from audit_selftest_edit import replace_literal_exactly_once

path = pathlib.Path(sys.argv[1])
replace_literal_exactly_once(
    path,
    "owned_base.is_some() || self.parametric_structs.contains_key(resolved_base)",
    "false || self.parametric_structs.contains_key(resolved_base) /* SelftestConstructorBaseNestedFieldOwner11369 */",
    label="Base-origin nested field registry selection",
)
PY
}

inject_constructor_base_concrete_identity_qualified() {
  injector_python "$1" "subset_julia_vm_compile/src/compile/context.rs" <<'PY'
import pathlib
import sys
from audit_selftest_edit import replace_literal_exactly_once

path = pathlib.Path(sys.argv[1])
replace_literal_exactly_once(
    path,
    "            base.to_string()\n",
    "            base_name.to_string() /* SelftestConstructorBaseConcreteIdentity11369 */\n",
    label="explicit Base concrete family identity",
)
PY
}

inject_constructor_base_type_expr_owner_erased() {
  injector_python "$1" "subset_julia_vm_compile/src/compile/expr/collection.rs" <<'PY'
import pathlib
import sys
from audit_selftest_edit import replace_literal_exactly_once

path = pathlib.Path(sys.argv[1])
replace_literal_exactly_once(
    path,
    "let resolved_base = if explicit_base_owner {",
    "let resolved_base = if false { // SelftestConstructorBaseTypeExprOwner11369",
    label="explicit Base type-expression owner",
)
PY
}

inject_constructor_compile_splat_owner_guard_disabled() {
  injector_python "$1" "subset_julia_vm_compile/src/compile/expr/call/mod.rs" <<'PY'
import pathlib
import sys
from audit_selftest_edit import replace_literal_exactly_once

path = pathlib.Path(sys.argv[1])
needle = """        if has_splat
            && !self.locals.contains_key(function)
            && !self.captured_vars.contains(function)
        {
"""
replacement = """        if false && has_splat
            && !self.locals.contains_key(function)
            && !self.captured_vars.contains(function)
        { // SelftestConstructorCompileSplatOwnerGuard11371
"""
replace_literal_exactly_once(
    path, needle, replacement, label="compile_call pre-splat constructor owner guard"
)
PY
}

inject_constructor_parametric_callee_capture_disabled() {
  injector_python "$1" "subset_julia_vm_types/src/ir/free_vars.rs" <<'PY'
import pathlib
import sys
from audit_selftest_edit import replace_literal_exactly_once

path = pathlib.Path(sys.argv[1])
needle = """            let callee_binding = parse_parametric_call(function)
                .map(|(base, _)| base)
                .unwrap_or_else(|| function.to_string());
"""
replacement = """            let callee_binding = function.to_string();
            // SelftestConstructorParametricCalleeCapture11373
"""
replace_literal_exactly_once(
    path, needle, replacement, label="parametric callee base capture"
)
PY
}

inject_constructor_dynamic_parametric_order_reversed() {
  injector_python "$1" "subset_julia_vm_compile/src/compile/expr/call/dynamic.rs" <<'PY'
import pathlib
import sys
from audit_selftest_edit import replace_literal_exactly_once

path = pathlib.Path(sys.argv[1])
source = path.read_text(encoding="utf-8")
type_args = """        let mut type_arg_temps = Vec::with_capacity(type_args.len());
        for type_arg in type_args {
            self.emit_parametric_type_arg_value(type_arg)?;
            let type_arg_temp = self.new_temp("dynamic_parametric_type_arg");
            self.emit(Instr::StoreAny(type_arg_temp.clone()));
            type_arg_temps.push(type_arg_temp);
        }
"""
reversed_order = """        for arg in args {
            self.compile_expr(arg)?;
        } // SelftestConstructorDynamicParametricOrder11375
        let mut type_arg_temps = Vec::with_capacity(type_args.len());
        for type_arg in type_args {
            self.emit_parametric_type_arg_value(type_arg)?;
            let type_arg_temp = self.new_temp("dynamic_parametric_type_arg");
            self.emit(Instr::StoreAny(type_arg_temp.clone()));
            type_arg_temps.push(type_arg_temp);
        }
"""
path.write_text(source, encoding="utf-8")
replace_literal_exactly_once(path, type_args, reversed_order, label="dynamic type-argument evaluation")
source = path.read_text(encoding="utf-8")
for block in (
    """            for arg in args {
                self.compile_expr(arg)?;
            }
""",
    """        for arg in args {
            self.compile_expr(arg)?;
        }
        for type_arg_temp in type_arg_temps {
""",
):
    if source.count(block) != 1:
        raise SystemExit("dynamic parametric self-test arg block drifted")
    replacement = "" if "for type_arg_temp" not in block else "        for type_arg_temp in type_arg_temps {\n"
    source = source.replace(block, replacement, 1)
path.write_text(source, encoding="utf-8")
PY
}

inject_constructor_runtime_splat_default_fallback_removed() {
  injector_python "$1" "subset_julia_vm_vm/src/vm/exec/call_function_variable.rs" <<'PY'
import pathlib
import re
import sys
from audit_selftest_edit import replace_regex_exactly_once

path = pathlib.Path(sys.argv[1])
pattern = re.compile(
    r"(?ms)(?P<context>if kwargs_map\.is_empty\(\) \{\s+"
    r"if let Value::DataType\(_\) = &func_val \{\s+)"
    r"if self\.try_construct_default_datatype\(\s*"
    r"&func_name,\s*&expanded_args,\s*\)\? \{"
)

def inject(match):
    return (
        match.group("context")
        + "if self.disabled_default_datatype_constructor(\n"
        + "                                            &func_name,\n"
        + "                                            &expanded_args,\n"
        + "                                        )? { // SelftestConstructorRuntimeSplatDefaultFallback11371"
    )

replace_regex_exactly_once(
    path, pattern, inject, label="kwargs-splat post-dispatch default constructor fallback"
)
PY
}

inject_binding_provenance_consumer_wildcard() {
  injector_python "$1" "subset_julia_vm_lowering/src/lowering/scope_bindings.rs" <<'PY'
import pathlib
import sys
from audit_selftest_edit import replace_literal_exactly_once

path = pathlib.Path(sys.argv[1])
needle = """                LocalDeclKind::CompilerEnclosing => {
                    self.compiler_enclosing.insert(var.clone());
                }
"""
replacement = """                _ => {
                    self.compiler_enclosing.insert(var.clone());
                } // SelftestBindingProvenanceWildcard11317
"""
replace_literal_exactly_once(
    path, needle, replacement, label="ScopeBindingInventory LocalDeclKind arm"
)
PY
}

inject_binding_provenance_aot_consumer_ignored() {
  injector_python "$1" "subset_julia_vm/src/aot/analyze/ir_converter/stmt.rs" <<'PY'
import pathlib
import sys
from audit_selftest_edit import replace_literal_exactly_once

path = pathlib.Path(sys.argv[1])
needle = """            Stmt::LocalDecl { kind, .. } => match kind {
                LocalDeclKind::Explicit | LocalDeclKind::CompilerEnclosing => Ok(vec![]),
            },
"""
replacement = """            Stmt::LocalDecl { .. } => Ok(vec![]),
            // SelftestBindingProvenanceAotIgnored11317
"""
replace_literal_exactly_once(
    path, needle, replacement, label="AoT convert_stmt_expanded LocalDeclKind arm"
)
PY
}

inject_binding_provenance_unclassified_if_let_consumer() {
  injector_python "$1" "subset_julia_vm_compile/src/compile/constants.rs" <<'PY'
import pathlib
import sys
from audit_selftest_edit import replace_literal_exactly_once

path = pathlib.Path(sys.argv[1])
needle = "fn stmt_contains_direct_throw(stmt: &Stmt) -> bool {"
replacement = """fn selftest_unclassified_local_decl_consumer_11317(stmt: &Stmt) -> bool {
    if let Stmt::LocalDecl { var, .. } = stmt {
        return !var.is_empty();
    }
    false
} // SelftestBindingProvenanceUnclassifiedConsumer11317

fn stmt_contains_direct_throw(stmt: &Stmt) -> bool {"""
replace_literal_exactly_once(
    path, needle, replacement, label="unclassified LocalDecl if-let consumer"
)
PY
}

inject_binding_provenance_authority_helper_corrupted() {
  injector_python "$1" "subset_julia_vm_compile/src/compile/core_compiler.rs" <<'PY'
import pathlib
import sys
from audit_selftest_edit import replace_literal_exactly_once

path = pathlib.Path(sys.argv[1])
needle = """    pub(super) fn emit_load_declared_global(&mut self, name: &str) {
        self.emit(Instr::LoadGlobalAny(
            self.declared_global_runtime_name(name),
        ));
    }
"""
replacement = """    pub(super) fn emit_load_declared_global(&mut self, name: &str) {
        let _qualified = self.declared_global_runtime_name(name);
        self.emit(Instr::LoadGlobalAny(name.to_owned()));
        // SelftestBindingProvenanceAuthorityHelperCorrupted11317
    }
"""
replace_literal_exactly_once(
    path, needle, replacement, label="declared-global load authority helper"
)
PY
}

inject_binding_provenance_bare_global_key() {
  injector_python "$1" "subset_julia_vm_compile/src/compile/expr/mod.rs" <<'PY'
import pathlib
import sys
from audit_selftest_edit import replace_literal_exactly_once

path = pathlib.Path(sys.argv[1])
needle = "            self.emit_load_declared_global(name);"
replacement = (
    "            let key = name.to_owned();\n"
    "            self.emit(Instr::LoadGlobalAny(key)); "
    "// SelftestBindingProvenanceBareGlobalKey11317"
)
replace_literal_exactly_once(
    path, needle, replacement, label="load_local declared-global authority call"
)
PY
}

# check_python_audit_compatibility.sh violations (Issue #11102): prove the
# floor model catches syntax, stdlib, and eager-annotation hazards separately.
inject_python_audit_newer_syntax() {
  cat > "$1/scripts/check_python_discovery_selftest.sh" <<'SH'
#!/usr/bin/env bash
python3 scripts/python_discovery_selftest.py
SH
  cat > "$1/scripts/python_discovery_selftest.py" <<'PY'
# audit-selftest SelftestPythonNewHelperDiscovery11102
match "selftest":
    case "selftest":
        pass
PY
}

inject_python_audit_option_bypass() {
  cat > "$1/scripts/check_python_option_bypass_selftest.sh" <<'SH'
#!/usr/bin/env bash
# audit-selftest SelftestPythonOptionBypass11102
HELPER=scripts/check_base_duplicate_signatures.py
python3 -I "$HELPER"
SH
}

inject_python_audit_newer_stdlib() {
  cat >> "$1/scripts/unsafe_inventory.py" <<'PY'

# audit-selftest SelftestPythonNewerStdlib11102
import tomllib
PY
}

inject_python_audit_eager_union() {
  injector_python "$1" "scripts/check_orphaned_rs_files.py" <<'PY'
import pathlib
import sys
from audit_selftest_edit import replace_literal_exactly_once

path = pathlib.Path(sys.argv[1])
needle = "from __future__ import annotations  # Python 3.9 evaluates `str | None` otherwise (#11093)."
replacement = "# audit-selftest SelftestPythonEagerUnion11102"
replace_literal_exactly_once(path, needle, replacement, label="future-annotations import")
PY
}

# check_builtin_type_registry.sh consumer disconnections and full-contract drift
# (Issue #10954). Each semantic consumer is severed independently; the fourth
# mutation deletes an unsampled registry row to prove whole-table coverage.
inject_builtin_registry_parser_disconnected() {
  injector_python "$1" "subset_julia_vm_types/src/types/julia_type/parsing.rs" <<'PY'
import pathlib
import sys
from audit_selftest_edit import replace_literal_exactly_once

path = pathlib.Path(sys.argv[1])
needle = '        if let Some(ty) = crate::types::builtin_type_for_parser(name) {'
replacement = (
    '        if let Some(ty) = None::<JuliaType> { '
    '// audit-selftest SelftestBuiltinRegistryParserDisconnected10954'
)
replace_literal_exactly_once(path, needle, replacement, label="parser registry delegation")
PY
}

inject_builtin_registry_compiler_disconnected() {
  injector_python "$1" "subset_julia_vm_compile/src/compile/expr/mod.rs" <<'PY'
import pathlib
import sys
from audit_selftest_edit import replace_literal_exactly_once

path = pathlib.Path(sys.argv[1])
needle = '                    if let Some(builtin_type) = crate::types::builtin_type_for_compiler(name) {'
replacement = (
    '                    if let Some(builtin_type) = None::<JuliaType> { '
    '// audit-selftest SelftestBuiltinRegistryCompilerDisconnected10954'
)
replace_literal_exactly_once(path, needle, replacement, label="compiler registry delegation")
PY
}

inject_builtin_registry_reflection_disconnected() {
  injector_python "$1" "subset_julia_vm_vm/src/vm/builtins_reflection/mod.rs" <<'PY'
import pathlib
import sys
from audit_selftest_edit import replace_literal_exactly_once

path = pathlib.Path(sys.argv[1])
needle = '        if let Some(authority) = builtin_type_binding_authority(field_name) {'
replacement = (
    '        if let Some(authority) = None::<BuiltinTypeBindingAuthority> { '
    '// audit-selftest SelftestBuiltinRegistryReflectionDisconnected10954'
)
replace_literal_exactly_once(path, needle, replacement, label="reflection registry delegation")
PY
}

inject_builtin_registry_entry_deleted() {
  injector_python "$1" "subset_julia_vm_types/src/types/builtin_type_registry.rs" <<'PY'
import pathlib
import sys
from audit_selftest_edit import replace_literal_exactly_once

path = pathlib.Path(sys.argv[1])
needle = '    builtin_type!("Xoshiro", Nominal("Xoshiro"), COMPILER),\n'
replacement = '    // audit-selftest SelftestBuiltinRegistryEntryDeleted10954\n'
replace_literal_exactly_once(path, needle, replacement, label="complete registry contract")
PY
}

log "=== audit-the-audits negative self-tests (Issues #9129 / #9388) ==="

# --- Original three (Issue #9129) ---
run_selftest "check_instr_wire_ids.sh (COVERAGE)" \
  "check_instr_wire_ids.sh" "SelftestBogusIntrinsic9129" inject_wire_ids \
  "SelftestBogusIntrinsic9129" "subset_julia_vm_bytecode/src/intrinsics.rs"

run_selftest "check_dispatch_determinism.sh (hash-iteration ratchet)" \
  "check_dispatch_determinism.sh" "call/mod.rs has 1 hash-collection iteration" inject_dispatch \
  "SelftestInjectedHashIter9129" "subset_julia_vm_compile/src/compile/expr/call/mod.rs"

run_selftest "check_dispatch_negative_oracle.sh (required MethodError cells)" \
  "check_dispatch_negative_oracle.sh" "missing required negative oracle case" \
  inject_dispatch_negative_oracle \
  "neg_vector_invariance_removed_9567" "subset_julia_vm/tests/fixtures/dispatch_parity/corpus.toml"

run_selftest "check_no_typevar_name_heuristic.sh (name-shape TypeVar heuristic)" \
  "check_no_typevar_name_heuristic.sh" "type-variable name heuristic remains" \
  inject_typevar_name_heuristic \
  "SelftestTypeVarNameHeuristic9563" "subset_julia_vm_types/src/types/julia_type/parsing.rs"

run_selftest "check_name_based_lookup.sh (new name-keyed TypeVar scope map)" \
  "check_name_based_lookup.sh" "typevar_scope_maps count grew" \
  inject_name_based_lookup \
  "SelftestNameBasedLookup10279" "subset_julia_vm_types/src/inference_core/type_core/match.rs"

run_selftest "check_name_based_lookup.sh (unclassified TypeVar/CoreType binding, Issue #10992)" \
  "check_name_based_lookup.sh" "typevar_core_bindings count grew from baseline 0 to 1" \
  inject_unclassified_typevar_core_binding \
  "SelftestUnclassifiedTypeVarCoreBinding10992" \
  "subset_julia_vm_types/src/inference_core/dispatch_resolver/core_match.rs"

run_selftest "check_name_based_lookup.sh (parallel where-binder scope stack)" \
  "check_name_based_lookup.sh" "lowering_binder_parallel_stacks count grew" \
  inject_parallel_where_binder_stack \
  "SelftestParallelWhereBinderStack10436" "subset_julia_vm_lowering/src/lowering/type_alias.rs"

run_selftest "check_name_based_lookup.sh (Main-owner resolver disconnected, Issue #11046)" \
  "check_name_based_lookup.sh" "missing semantic-identity anchor for Main-owner recovery" \
  inject_name_based_lookup_main_owner_disconnect \
  "SelftestMainOwnerDisconnect11046" "subset_julia_vm_bytecode/src/struct_registry.rs"

run_selftest "check_name_based_lookup.sh (cache owner restore disconnected, Issue #11046)" \
  "check_name_based_lookup.sh" "missing semantic-identity anchor for cache-restored owner-aware declaration insertion" \
  inject_name_based_lookup_cache_owner_disconnect \
  "struct_table.insert(" "subset_julia_vm_compile/src/compile/cache.rs"

run_selftest "check_exception_taxonomy_funnel.sh (message names a class the variant contradicts)" \
  "check_exception_taxonomy_funnel.sh" "carries a message that opens with" \
  inject_exception_taxonomy_message_class \
  "SelftestExceptionTaxonomyFunnel11146" "subset_julia_vm_vm/src/vm/builtins_types.rs"

run_selftest "check_exception_taxonomy_funnel.sh (catch-time builder hard-codes a struct name)" \
  "check_exception_taxonomy_funnel.sh" "hard-codes the exception struct-name literal" \
  inject_exception_taxonomy_hardcoded_name \
  "SelftestExceptionTaxonomyHardcodedName11146" "subset_julia_vm_vm/src/vm/exec/error_handling.rs"

run_selftest "check_exception_taxonomy_funnel.sh (catch-all arm in the funnel)" \
  "check_exception_taxonomy_funnel.sh" "catch-all arm in VmError::exception_class()" \
  inject_exception_taxonomy_catch_all \
  "SelftestExceptionTaxonomyCatchAll11146" "subset_julia_vm_bytecode/src/error.rs"

run_selftest "check_exception_taxonomy_funnel.sh (new Julia-layer error(\"<Class>: ...\") raise)" \
  "check_exception_taxonomy_funnel.sh" "new \`error(" \
  inject_exception_taxonomy_julia_error_class \
  "SelftestExceptionTaxonomyJuliaError11146" "subset_julia_vm/src/julia/base/some.jl"

run_selftest "audit_exception_payload_carrier.sh (ad-hoc pending field, Issue #11647)" \
  "audit_exception_payload_carrier.sh" "ad-hoc exception payload carrier" \
  inject_exception_payload_ad_hoc_field \
  "SelftestExceptionPayloadCarrier11647" "subset_julia_vm_vm/src/vm/mod.rs"

run_selftest "audit_exception_payload_carrier.sh (consume after classification, Issue #11647)" \
  "audit_exception_payload_carrier.sh" \
  "exception funnel must consume payload before exception classification" \
  inject_exception_payload_consume_after_classification \
  "SelftestExceptionPayloadConsumeLate11647" \
  "subset_julia_vm_vm/src/vm/exec/error_handling.rs"

run_selftest "audit_exception_payload_carrier.sh (carrier naming bypass, Issue #11647)" \
  "audit_exception_payload_carrier.sh" \
  "unreviewed Vm field can carry exception values" \
  inject_exception_payload_name_bypass_field \
  "SelftestExceptionPayloadNameBypass11647" "subset_julia_vm_vm/src/vm/mod.rs"

run_selftest "check_type_representation_string_reparse.sh (new semantic JuliaType::from_name)" \
  "check_type_representation_string_reparse.sh" "SelftestTypeStringReparse10460" \
  inject_type_representation_string_reparse \
  "SelftestTypeStringReparse10460" "subset_julia_vm_types/src/inference_core/type_core/match.rs"

run_selftest "check_type_representation_string_reparse.sh (function-item alias)" \
  "check_type_representation_string_reparse.sh" "SelftestTypeStringReparseAlias10460" \
  inject_type_representation_string_reparse_alias \
  "SelftestTypeStringReparseAlias10460" "subset_julia_vm_types/src/inference_core/type_core/match.rs"

run_selftest "check_type_representation_string_reparse.sh (same-count site substitution)" \
  "check_type_representation_string_reparse.sh" "exact site inventory changed" \
  inject_type_reparse_same_count_substitution \
  "SelftestTypeReparseInventory10460" "subset_julia_vm_vm/src/vm/builtins_types.rs"

run_selftest "check_type_representation_string_reparse.sh (below-baseline inventory drift)" \
  "check_type_representation_string_reparse.sh" "is below baseline" \
  inject_type_reparse_below_baseline_drift \
  "SelftestTypeReparseBelowBaseline10460" "subset_julia_vm_vm/src/vm/builtins_types.rs"

run_selftest "check_type_representation_string_reparse.sh (cfg(test) trivia regression, Issue #11208)" \
  "check_type_representation_string_reparse.sh" "cfg(test) trivia self-test leaked test-only token" \
  inject_type_reparse_cfg_test_trivia_regression \
  "SELFTEST11208-CFG-TRIVIA" "scripts/check_type_representation_string_reparse.sh"

run_selftest "check_builtin_type_registry.sh (parser consumer disconnected, Issue #10954)" \
  "check_builtin_type_registry.sh" "JuliaType::from_name lost its canonical parser projection" \
  inject_builtin_registry_parser_disconnected \
  "SelftestBuiltinRegistryParserDisconnected10954" \
  "subset_julia_vm_types/src/types/julia_type/parsing.rs"

run_selftest "check_builtin_type_registry.sh (compiler consumer disconnected, Issue #10954)" \
  "check_builtin_type_registry.sh" "compiler Expr::Var lost canonical type-object emission" \
  inject_builtin_registry_compiler_disconnected \
  "SelftestBuiltinRegistryCompilerDisconnected10954" \
  "subset_julia_vm_compile/src/compile/expr/mod.rs"

run_selftest "check_builtin_type_registry.sh (reflection consumer disconnected, Issue #10954)" \
  "check_builtin_type_registry.sh" "module isdefined lost its canonical reflection projection" \
  inject_builtin_registry_reflection_disconnected \
  "SelftestBuiltinRegistryReflectionDisconnected10954" \
  "subset_julia_vm_vm/src/vm/builtins_reflection/mod.rs"

run_selftest "check_builtin_type_registry.sh (unsampled registry entry deleted, Issue #10954)" \
  "check_builtin_type_registry.sh" "canonical builtin registry contract drifted" \
  inject_builtin_registry_entry_deleted \
  "SelftestBuiltinRegistryEntryDeleted10954" \
  "subset_julia_vm_types/src/types/builtin_type_registry.rs"

run_selftest "inventory_rust_semantics.sh (enum/table parse mismatch)" \
  "inventory_rust_semantics.sh" "parse mismatch" inject_inventory \
  "SelftestBogusBuiltin9129" "subset_julia_vm_bytecode/src/builtins.rs" --summary

# --- Allowlist / zero-match carriers (Issue #9388) ---
run_selftest "check_value_array_allowlist.sh (Value::Array zero-match)" \
  "check_value_array_allowlist.sh" "unexpected Value::Array use" inject_value_array \
  "selftest9388" "subset_julia_vm_vm/src/vm/mod.rs"

run_selftest "check_memory_to_array_ref_allowlist.sh (zero-match)" \
  "check_memory_to_array_ref_allowlist.sh" "was reintroduced" inject_memory_to_array_ref \
  "selftest9388" "subset_julia_vm_vm/src/vm/mod.rs"

run_selftest "check_complex_interleaved_allowlist.sh (containment)" \
  "check_complex_interleaved_allowlist.sh" "builtins_io.rs" inject_complex_interleaved \
  "selftest9388" "subset_julia_vm_vm/src/vm/builtins_io.rs"

run_selftest "check_native_value_ops_resolve_structref.sh (membership routing)" \
  "check_native_value_ops_resolve_structref.sh" "membership" inject_native_value_ops \
  "values_equal_for_membershipSELFTEST9388" "subset_julia_vm_vm/src/vm/builtins_types.rs"

# Type-wall variant (Issue #8919): an un-witnessed sink (raw &Value) must be
# rejected by the type-anchored audit, naming the sink.
run_selftest "check_native_value_ops_resolve_structref.sh (un-witnessed sink)" \
  "check_native_value_ops_resolve_structref.sh" "does not require the StructResolved witness" \
  inject_native_value_ops_witness \
  "fn egal_compare_witnessed(left: &Value" "subset_julia_vm_vm/src/vm/builtins_equality.rs"

# --- Ratchet-style audits (Issue #9388) ---
run_selftest "check_structural_debt_inventory.sh (stale closed-issue TODO)" \
  "check_structural_debt_inventory.sh" "closed-issue TODO references" inject_structural_debt \
  "marker9388" "subset_julia_vm_vm/src/vm/mod.rs"

run_selftest "check_definition_order_merges.sh (raw independent fragment bypass)" \
  "check_definition_order_merges.sh" "definition-order merge inventory drift" \
  inject_definition_order_merge_bypass \
  "SelftestDefinitionOrderBypass11036" "subset_julia_vm/src/pipeline.rs"

run_selftest "check_definition_order_merges.sh (aliased vector bypass)" \
  "check_definition_order_merges.sh" "definition-order merge inventory drift" \
  inject_definition_order_aliased_merge_bypass \
  "SelftestDefinitionOrderAliasBypass11036" "subset_julia_vm/src/pipeline.rs"

run_selftest "check_definition_order_merges.sh (renamed cursor inventory drift)" \
  "check_definition_order_merges.sh" "definition-order merge inventory drift" \
  inject_definition_order_renamed_cursor_site \
  "SelftestDefinitionOrderRenamedCursor11036" "subset_julia_vm/src/pipeline.rs"

run_selftest "check_definition_order_merges.sh (runtime nominal state row removed, Issue #11740)" \
  "check_definition_order_merges.sh" \
  "missing runtime-state inventory row 'runtime_nominals:runtime_nominal_activations'" \
  inject_definition_order_runtime_nominal_row_removed \
  "runtime_nominal_activations" \
  "docs/vm/DEFINITION_ORDER_MERGE_INVENTORY.tsv"

run_selftest "check_callable_singleton_identity.sh (FunctionValue authority accessor removed, Issue #11703)" \
  "check_callable_singleton_identity.sh" \
  "FunctionValue lost its singleton_identity authority accessor" \
  inject_callable_singleton_identity_accessor_removed \
  "singleton_identity_removed_11703" \
  "subset_julia_vm_bytecode/src/value/metadata.rs"

run_selftest "check_rust_semantics_ratchet.sh (perf-pending row)" \
  "check_rust_semantics_ratchet.sh" "perf-pending row count grew" inject_rust_semantics_ratchet \
  "SelftestPerfPending9388" "docs/vm/rust_semantics_classification.tsv"

run_selftest "check_numeric_matrix_full_allowlist.sh (zero-residual ratchet)" \
  "check_numeric_matrix_full_allowlist.sh" "numeric matrix full allowlist has non-header rows" inject_numeric_matrix_full_allowlist \
  "selftest injected residual row" "docs/vm/NUMERIC_MATRIX_FULL_ALLOWLIST.tsv"

run_selftest "check_generator_trait_matrix.sh (skiplist row ratchet)" \
  "check_generator_trait_matrix.sh" "generator trait matrix skiplist grew" inject_generator_trait_matrix_skiplist_row_growth \
  "SelftestGeneratorTraitSkiplistGrow9388" "docs/vm/GENERATOR_TRAIT_MATRIX_SKIPLIST.tsv"

run_selftest "check_panic_free_ratchet.sh (new panic-source module)" \
  "check_panic_free_ratchet.sh" "selftest_panic_ratchet_9388" inject_panic_free_ratchet \
  "selftest_panic_ratchet_9388" "subset_julia_vm/src/selftest_panic_ratchet_9388.rs"

# check_panic_free_production_baseline.sh (Issue #10908 Phase 3 of #10869):
# the same injected file is a brand-new, non-test, non-build-time,
# non-cache-boundary source file with a real top-level `.unwrap()` — the
# classifier defaults an unrecognized file to `user-input-reachable`
# ("production"), and it is not in docs/vm/PANIC_FREE_PRODUCTION_BASELINE.tsv,
# so the gate must reject it as a NEW unallowlisted production panic site
# (distinct reason text from the raw ratchet above, proving this gate's OWN
# bucket-aware guard code fires, not just the pre-existing raw ratchet).
run_selftest "check_panic_free_production_baseline.sh (new unallowlisted production panic site)" \
  "check_panic_free_production_baseline.sh" "NEW unallowlisted user-input-reachable" inject_panic_free_ratchet \
  "selftest_panic_ratchet_9388" "subset_julia_vm/src/selftest_panic_ratchet_9388.rs"

# --- Guarded-premerge gate-ownership regression (Issue #10870) ---
#
# #8740 and #9920-#9925 recurred because check_structural_debt_inventory.sh
# and check_panic_free_ratchet.sh were registered in ci.yml but NEVER wired
# into scripts/premerge_gate.sh's default gate set — while Actions is
# disabled, that meant nothing local ever ran them, so both drifted red on
# `main` without any guarded merge catching it. This proves the fix holds:
# scripts/run_source_only_audits.sh (the runner premerge_gate.sh's default
# gate list invokes, driven by the scripts/source_only_audits.tsv registry)
# still fails, AND names the failing audit, when a registered ratchet goes
# red — reusing the exact panic-free-ratchet injection above so a clean
# sandbox is green (both ratchets pass) and the injected sandbox is red for
# an injection-specific reason, not a coincidental drifted baseline.
run_selftest "run_source_only_audits.sh (default premerge gate registry, Issue #10870)" \
  "run_source_only_audits.sh" "selftest_panic_ratchet_9388" inject_panic_free_ratchet \
  "selftest_panic_ratchet_9388" "subset_julia_vm/src/selftest_panic_ratchet_9388.rs"

run_selftest "run_source_only_audits.sh (compiler/VM boundary registered, Issue #11065)" \
  "run_source_only_audits.sh" "selftest9388" inject_compile_vm_coupling \
  "selftest9388" "subset_julia_vm_vm/src/vm/mod.rs"

run_selftest "run_source_only_audits.sh (fixture categories registered, Issue #11065)" \
  "run_source_only_audits.sh" "arrays_selftest9671" inject_fixture_category \
  "SelftestUnlistedCategory9671" "subset_julia_vm/tests/fixtures/arrays_selftest9671/marker.txt"

run_selftest "run_source_only_audits.sh (audit registration completeness registered, Issue #11065)" \
  "run_source_only_audits.sh" "check_selftest_unregistered_example.sh" \
  inject_unregistered_audit \
  "SelftestUnregisteredAudit11065" "scripts/check_selftest_unregistered_example.sh"

run_selftest "run_source_only_audits.sh (STATUS/DONE overflow reaches registered checker, Issue #11263)" \
  "run_source_only_audits.sh" "source-only audit 'status_done_archive_budget'" \
  inject_status_archive_budget_overflow \
  "SelftestStatusArchiveBudget11263" "docs/vm/STATUS.md"

run_selftest "check_source_only_audit_sync.sh (executable default runner path, Issue #11065)" \
  "check_source_only_audit_sync.sh" \
  "default gate list does not include the exact source-only runner command" \
  inject_premerge_runner_command_removed \
  "SelftestPremergeRunnerRemoved11065" "scripts/premerge_gate.sh"

run_selftest "check_source_only_audit_sync.sh (required compiler/VM row, Issue #11065)" \
  "check_source_only_audit_sync.sh" "missing required default registry row 'compile_vm_coupling'" \
  inject_compile_vm_registry_row_removed \
  "SelftestCompileVmRegistryRemoved11065" "scripts/source_only_audits.tsv"

run_selftest "check_source_only_audit_sync.sh (required fixture-category row, Issue #11065)" \
  "check_source_only_audit_sync.sh" "missing required default registry row 'fixture_categories'" \
  inject_fixture_categories_registry_row_removed \
  "SelftestFixtureRegistryRemoved11065" "scripts/source_only_audits.tsv"

run_selftest "check_source_only_audit_sync.sh (required fixture-coverage self-test row, Issue #11041)" \
  "check_source_only_audit_sync.sh" "missing required default registry row 'fixture_coverage_contract_selftest'" \
  inject_fixture_coverage_selftest_registry_row_removed \
  "SelftestFixtureCoverageRegistryRemoved11041" "scripts/source_only_audits.tsv"

run_selftest "check_source_only_audit_sync.sh (required definition-order row, Issue #11036)" \
  "check_source_only_audit_sync.sh" "missing required default registry row 'definition_order_merges'" \
  inject_definition_order_registry_row_removed \
  "SelftestDefinitionOrderRegistryRemoved11036" "scripts/source_only_audits.tsv"

run_selftest "check_source_only_audit_sync.sh (required STATUS/DONE row removed, Issue #11263)" \
  "check_source_only_audit_sync.sh" "missing required default registry row 'status_done_archive_budget'" \
  inject_status_done_archive_registry_row_removed \
  "SelftestStatusDoneRegistryRemoved11263" "scripts/source_only_audits.tsv"

run_selftest "check_source_only_audit_sync.sh (required STATUS/DONE row weakened, Issue #11263)" \
  "check_source_only_audit_sync.sh" "missing required default registry row 'status_done_archive_budget'" \
  inject_status_done_archive_registry_row_weakened \
  "SelftestStatusDoneRegistryWeakened11263" "scripts/source_only_audits.tsv"

run_selftest "check_source_only_audit_sync.sh (required Base cache schema row, Issue #10688)" \
  "check_source_only_audit_sync.sh" \
  "missing required default registry row 'base_cache_schema_fingerprint'" \
  inject_base_cache_schema_registry_row_weakened \
  "SelftestBaseCacheSchemaRegistryWeakened10688" "scripts/source_only_audits.tsv"

run_selftest "check_source_only_audit_sync.sh (required exception payload row, Issue #11647)" \
  "check_source_only_audit_sync.sh" \
  "missing required default registry row 'exception_payload_carrier'" \
  inject_exception_payload_registry_row_weakened \
  "SelftestExceptionPayloadRegistryWeakened11647" "scripts/source_only_audits.tsv"

run_selftest "check_source_only_audit_sync.sh (required constructor-owner row, Issue #11172)" \
  "check_source_only_audit_sync.sh" \
  "missing required default registry row 'constructor_owner_resolution'" \
  inject_constructor_owner_registry_row_weakened \
  "SelftestConstructorOwnerRegistryWeakened11172" "scripts/source_only_audits.tsv"

run_selftest "check_source_only_audit_sync.sh (required constructor-return row, Issue #11436)" \
  "check_source_only_audit_sync.sh" \
  "missing required default registry row 'constructor_return_identity'" \
  inject_constructor_return_registry_row_weakened \
  "SelftestConstructorReturnRegistryWeakened11436" "scripts/source_only_audits.tsv"

run_selftest "check_source_only_audit_sync.sh (required binding-provenance row, Issue #11317)" \
  "check_source_only_audit_sync.sh" \
  "missing required default registry row 'binding_provenance_authority'" \
  inject_binding_provenance_registry_row_weakened \
  "SelftestBindingProvenanceRegistryWeakened11317" "scripts/source_only_audits.tsv"

run_selftest "check_source_only_audit_sync.sh (required Base-export row, Issue #11298)" \
  "check_source_only_audit_sync.sh" \
  "missing required default registry row 'base_exports_subset'" \
  inject_base_exports_registry_row_weakened \
  "SelftestBaseExportRegistryWeakened11298" "scripts/source_only_audits.tsv"

run_selftest "check_source_only_audit_sync.sh (required source-position row, Issue #11100)" \
  "check_source_only_audit_sync.sh" \
  "missing required default registry row 'source_position_chronology'" \
  inject_source_position_registry_row_weakened \
  "SelftestSourcePositionRegistryWeakened11100" "scripts/source_only_audits.tsv"

run_selftest "check_source_only_audit_sync.sh (registry boolean schema, Issue #11065)" \
  "check_source_only_audit_sync.sh" "invalid premerge_default='ture'" \
  inject_registry_boolean_typo \
  "SelftestRegistryBooleanTypo11065" "scripts/source_only_audits.tsv"

run_selftest "check_source_only_audit_sync.sh (executable CI path, Issue #11065)" \
  "check_source_only_audit_sync.sh" "no executable 'run: bash scripts/check_fixture_categories.sh' step" \
  inject_ci_executable_step_removed \
  "SelftestCiExecutableStepRemoved11065" ".github/workflows/ci.yml"

run_selftest "check_build_preload_packages_explicit.sh (explicit-only preload default, Issue #11055)" \
  "check_build_preload_packages_explicit.sh" "should assign PRELOAD_PACKAGES_FOR_BUILD=\"\"" \
  inject_build_preload_packages_explicit \
  "selftest11055_implicit_preload_default" "build.sh"

run_selftest "check_build_locked.sh (multiline unlocked build, Issue #11257)" \
  "check_build_locked.sh" "MISSING --locked:" \
  inject_build_locked_multiline_unlocked \
  "SelftestBuildLockedMultiline11257" "scripts/check_build_locked.sh" \
  scripts/check_build_locked.sh

# --- Test-binary budget + fixture categories (Issue #9671) ---
run_selftest "check_test_binary_budget.sh (unlisted test binary)" \
  "check_test_binary_budget.sh" "unapproved test binary" inject_test_binary_budget \
  "SelftestUnlistedBinary9671" "subset_julia_vm/tests/selftest_unlisted_9671_tests.rs"

run_selftest "check_fixture_categories.sh (unlisted category dir)" \
  "check_fixture_categories.sh" "unapproved fixture category" inject_fixture_category \
  "SelftestUnlistedCategory9671" "subset_julia_vm/tests/fixtures/arrays_selftest9671/marker.txt"

run_selftest "check_audit_negative_selftest.sh --registration-only (unregistered audit, Issue #11065)" \
  "check_audit_negative_selftest.sh" \
  "check_selftest_unregistered_example.sh has neither a negative self-test" \
  inject_unregistered_audit \
  "SelftestUnregisteredAudit11065" "scripts/check_selftest_unregistered_example.sh" \
  --registration-only

run_selftest "check_status_done_archive_budget.sh (live STATUS/DONE budget, Issue #11263)" \
  "check_status_done_archive_budget.sh" "STATUS.md has" \
  inject_status_archive_budget_overflow \
  "SelftestStatusArchiveBudget11263" "docs/vm/STATUS.md"

run_selftest "check_status_done_archive_budget.sh (live DONE budget independently, Issue #11263)" \
  "check_status_done_archive_budget.sh" "DONE.md has" \
  inject_done_archive_budget_overflow \
  "SelftestDoneArchiveBudget11263" "docs/vm/DONE.md"

run_selftest "check_julia_display_write_text_paths.sh (display text through binary write)" \
  "check_julia_display_write_text_paths.sh" "display helper writes arbitrary arg through binary write" \
  inject_julia_display_write_text_paths \
  "SelftestDisplayWriteTextPath10008" "subset_julia_vm/src/julia/base/io.jl"

run_selftest "check_error_span_ratchet.sh (new span-less error module)" \
  "check_error_span_ratchet.sh" "selftest_errspan_9388" inject_error_span_ratchet \
  "selftest_errspan_9388" "subset_julia_vm/src/selftest_errspan_9388.rs"

run_selftest "audit_compile_vm_coupling.sh (vm -> compile import)" \
  "audit_compile_vm_coupling.sh" "selftest9388" inject_compile_vm_coupling \
  "selftest9388" "subset_julia_vm_vm/src/vm/mod.rs"

run_selftest "audit_compile_vm_coupling.sh (compile test -> vm import)" \
  "audit_compile_vm_coupling.sh" "crate::vm::Vm" inject_compile_test_vm_coupling \
  "selftest9808_compile_test_imports_vm" "subset_julia_vm_compile/src/compile/mod.rs"

run_selftest "audit_base_cache_schema_fingerprint.sh (manifest integrity)" \
  "audit_base_cache_schema_fingerprint.sh" "references missing file" inject_base_cache_fingerprint \
  "selftest_missing_9388" "subset_julia_vm_compile/src/compile/base_cache_schema_files.txt"

run_selftest "audit_base_cache_schema_fingerprint.sh (listed source drift, Issue #10688)" \
  "audit_base_cache_schema_fingerprint.sh" "Base cache schema fingerprint changed" \
  inject_base_cache_listed_file_drift \
  "SelftestBaseCacheListedFileDrift10688" \
  "subset_julia_vm_compile/src/compile/instr_wire_ids.rs"

# --- Additional green static-grep audits (Issue #9388) ---
run_selftest "check_call_handler_kwargs.sh (inline kwparam loop)" \
  "check_call_handler_kwargs.sh" "must use bind_kwargs_defaults" inject_call_handler_kwargs \
  "selftest9388" "subset_julia_vm_vm/src/vm/exec/mod.rs"

run_selftest "check_audit_scripts_bash3_compat.sh (bash-4 associative array)" \
  "check_audit_scripts_bash3_compat.sh" "uses a bash 4+ construct" inject_bash3_compat \
  "selftest9388" "scripts/check_div_specializations.sh"

run_selftest "check_builtin_duplicates.sh (duplicate BuiltinId)" \
  "check_builtin_duplicates.sh" "SelftestDuplicateMarker" inject_builtin_duplicates \
  "SelftestDuplicateMarker" "subset_julia_vm_vm/src/vm/builtins_io.rs"

run_selftest "check_no_expect_in_bin.sh (.expect in bin)" \
  "check_no_expect_in_bin.sh" "found in bin" inject_no_expect_in_bin \
  "selftest9388" "subset_julia_vm/src/bin/sjulia.rs"

run_selftest "check_div_specializations.sh (missing width)" \
  "check_div_specializations.sh" "reintroduced concrete same-type div specializations" inject_div_specializations \
  "selftest9388_concrete_div" "subset_julia_vm/src/julia/base/int.jl"

run_selftest "check_promote_builtin_no_tuple_fallback.sh (tuple fallback)" \
  "check_promote_builtin_no_tuple_fallback.sh" "Promote builtin fallback must not construct Value::Tuple" \
  inject_promote_builtin_tuple_fallback \
  "selftest9896_promote_tuple_fallback" "subset_julia_vm_vm/src/vm/builtins_exec.rs"

run_selftest "check_array_constructor_memory_first.sh (open-coded undef)" \
  "check_array_constructor_memory_first.sh" "ArrayValue::undef_typed" inject_array_constructor_memory_first \
  "selftest9388" "subset_julia_vm_vm/src/vm/builtins_arrays.rs"

run_selftest "check_no_hardcoded_var_names_in_inference.sh (new hardcoded name)" \
  "check_no_hardcoded_var_names_in_inference.sh" "SelftestStruct9388" inject_no_hardcoded_var_names \
  "SelftestStruct9388" "subset_julia_vm_compile/src/compile/expr/infer/array.rs"

run_selftest "check_no_public_base_stdlib_routes.sh (Base.<stdlib> route)" \
  "check_no_public_base_stdlib_routes.sh" "string route" inject_no_public_base_stdlib \
  "selftest9388" "subset_julia_vm_compile/src/compile/core_compiler.rs"

run_selftest "check_generated_files.sh (missing Re-generate comment)" \
  "check_generated_files.sh" "Re-generate with" inject_generated_files \
  "selftest9388" "subset_julia_vm/src/selftest_generated_9388.rs"

# --- Remaining green static-grep Memory-first audits (Issue #9463) ---
run_selftest "check_array_literal_memory_first.sh (open-coded TypedArrayValue)" \
  "check_array_literal_memory_first.sh" "constructs TypedArrayValue directly" \
  inject_array_literal_memory_first \
  "selftest9463" "subset_julia_vm_vm/src/vm/exec/array_basic.rs"

run_selftest "check_broadcast_hof_memory_first.sh (open-coded result builder)" \
  "check_broadcast_hof_memory_first.sh" "must use ArrayValue::memory_first_from_* helpers" \
  inject_broadcast_hof_memory_first \
  "selftest9463" "subset_julia_vm_vm/src/vm/broadcast.rs"

run_selftest "check_collect_memory_first.sh (non-Memory-first collect(tuple))" \
  "check_collect_memory_first.sh" "materializes collect(tuple) without Memory-first helpers" \
  inject_collect_memory_first \
  "selftest9463" "subset_julia_vm_vm/src/vm/type_ops/iteration.rs"

# --- Repointed drifted-target checks + moved-target guard (Issue #9573) ---
run_selftest "check_collect_memory_first.sh (non-Memory-first collect(range), repointed target)" \
  "check_collect_memory_first.sh" "materializes collect(range) without Memory-first helpers" \
  inject_collect_range_memory_first \
  "selftest9573" "subset_julia_vm_bytecode/src/value/range.rs"

run_selftest "check_collect_memory_first.sh (moved audit target file)" \
  "check_collect_memory_first.sh" "audit target file missing" \
  inject_collect_memory_first_moved_target \
  "selftest9573moved" "subset_julia_vm_bytecode/src/value/range_selftest9573_moved.rs"

run_selftest "check_literal_repl_memory_first.sh (non-Memory-first Literal::Array*)" \
  "check_literal_repl_memory_first.sh" "Literal::Array* conversion must use ArrayValue::memory_first_from_* helpers" \
  inject_literal_repl_memory_first \
  "selftest9463" "subset_julia_vm_compile/src/compile/expr/mod.rs"

# --- Remaining green static-grep registry / boundary audits (Issue #9463) ---
run_selftest "check_base_routing_registry.sh (empty upstream_ref)" \
  "check_base_routing_registry.sh" "empty upstream_ref" inject_base_routing_registry \
  "selftest9463" "subset_julia_vm_compile/src/compile/base_functions.rs"

run_selftest "check_no_new_domain_builtins.sh (BuiltinId count ratchet)" \
  "check_no_new_domain_builtins.sh" "SelftestDomainBuiltin9463" inject_no_new_domain_builtins \
  "SelftestDomainBuiltin9463" "subset_julia_vm_bytecode/src/builtins.rs"

run_selftest "check_no_new_domain_builtins.sh (Layer-2 LOC ratchet)" \
  "check_no_new_domain_builtins.sh" "builtins_selftest9892loc.rs" inject_no_new_domain_builtins_loc \
  "selftest9892loc" "subset_julia_vm_vm/src/vm/builtins_selftest9892loc.rs"

run_selftest "check_unsafe_inventory.sh (new unannotated unsafe)" \
  "check_unsafe_inventory.sh" "selftest9463unsafe" inject_unsafe_inventory \
  "selftest9463unsafe" "subset_julia_vm_vm/src/vm/mod.rs"

run_selftest "check_workarounds_documented.sh (workaround without Issue link)" \
  "check_workarounds_documented.sh" "without an Issue link" inject_workarounds_documented \
  "selftest9463" "subset_julia_vm_vm/src/vm/mod.rs"

run_selftest "check_workarounds_sync.sh (Issue not in WORKAROUNDS.md)" \
  "check_workarounds_sync.sh" "does not appear in" inject_workarounds_sync \
  "99999463" "subset_julia_vm_vm/src/vm/mod.rs"

run_selftest "audit_julia_base_stubs.sh (unmarked trivial untyped stub)" \
  "audit_julia_base_stubs.sh" "selftest9463stub" inject_julia_base_stubs \
  "selftest9463stub" "subset_julia_vm/src/julia/base/bool.jl"

# --- Red-on-main (#8740) audits: injection-specific unique-marker reasons ---
run_selftest "check_missing_debug.sh (public type missing Debug)" \
  "check_missing_debug.sh" "Selftest9463NoDebug" inject_missing_debug \
  "Selftest9463NoDebug" "subset_julia_vm_compile/src/compile/core_compiler.rs"

run_selftest "check_array_public_data_access.sh (raw try_data_f64 read)" \
  "check_array_public_data_access.sh" "selftest9463arraypub" inject_array_public_data_access \
  "selftest9463arraypub" "subset_julia_vm_vm/src/vm/broadcast.rs"

run_selftest "check_array_public_data_access.sh (generator public getindex materialization)" \
  "check_array_public_data_access.sh" "selftest9735genindex" inject_generator_public_indexing_materialization \
  "selftest9735genindex" "subset_julia_vm_vm/src/vm/exec/array_index.rs"

run_selftest "check_array_public_data_access.sh (removed shared-parent anchor, repointed target)" \
  "check_array_public_data_access.sh" "classify reshape shared-parent arrays" \
  inject_array_shared_parent_anchor \
  "selftest9573_parent" "subset_julia_vm_bytecode/src/value/array_value/access.rs"

run_selftest "check_binary_both_fallback_inventory.sh (undocumented tag)" \
  "check_binary_both_fallback_inventory.sh" "Selftest9463bbtag" inject_binary_both_fallback \
  "Selftest9463bbtag" "subset_julia_vm_vm/src/vm/exec/binary_both.rs"

run_selftest "check_collect_fallback_inventory.sh (undocumented tag)" \
  "check_collect_fallback_inventory.sh" "Selftest9463cftag" inject_collect_fallback \
  "Selftest9463cftag" "subset_julia_vm_vm/src/vm/builtins_exec.rs"

run_selftest "check_vmerror_classification.sh (unannotated TypeError block)" \
  "check_vmerror_classification.sh" "selftest9463_vmerr" inject_vmerror_classification \
  "VmError::TypeError" "subset_julia_vm_vm/src/vm/exec/selftest9463_vmerr.rs"

run_selftest "check_no_panic_in_tests.sh (=> panic! in src)" \
  "check_no_panic_in_tests.sh" "selftest9463_panic" inject_no_panic_in_tests \
  "=> panic!" "subset_julia_vm/src/selftest9463_panic.rs"

run_selftest "audit_binary_dispatch_single_source.sh (removed resolver adapter anchor)" \
  "audit_binary_dispatch_single_source.sh" "FAIL resolver adapter covers Add" \
  inject_binary_dispatch_single_source \
  "resolverADAPTER9463removed" "subset_julia_vm_compile/src/compile/expr/binary/mod.rs"

run_selftest "audit_binary_dispatch_single_source.sh (removed BinaryStaticVerdict anchor, repointed target)" \
  "audit_binary_dispatch_single_source.sh" "FAIL BinaryStaticVerdict enum declared" \
  inject_binary_dispatch_resolver_anchor \
  "SelftestVerdict9573" "subset_julia_vm_types/src/inference_core/dispatch_resolver.rs"

run_selftest "check_call_function_variable_value_dispatch_order.sh (local legacy scorer bypasses shared resolver)" \
  "check_call_function_variable_value_dispatch_order.sh" \
  "calls self.dispatch_function_variable() directly at line" \
  inject_call_function_variable_dispatch_order \
  "selftest9987_local_legacy" "subset_julia_vm_vm/src/vm/exec/call_function_variable.rs"

run_selftest "check_call_function_variable_value_dispatch_order.sh (comment cannot impersonate shared call)" \
  "check_call_function_variable_value_dispatch_order.sh" \
  "never calls dispatch_function_variable_for_values()" \
  inject_call_function_variable_fake_shared_comment \
  "SELFTEST10461-COMMENT" "subset_julia_vm_vm/src/vm/exec/call_function_variable.rs"

run_selftest "check_call_function_variable_value_dispatch_order.sh (CallDynamic ignores carried callee identity)" \
  "check_call_function_variable_value_dispatch_order.sh" \
  "Instr::CallDynamic must consume operands.callee_name" \
  inject_call_dynamic_callee_identity_ignored \
  "selftest10461-anonymous" "subset_julia_vm_vm/src/vm/exec/call_dynamic.rs"

run_selftest "check_call_function_variable_value_dispatch_order.sh (invoke declared signature enters runtime refinement)" \
  "check_call_function_variable_value_dispatch_order.sh" \
  "declared-signature dispatch must not use value-based runtime refinement" \
  inject_invoke_declared_signature_runtime_refinement \
  "selftest11619 runtime refinement" "subset_julia_vm_vm/src/vm/exec/call_function_variable.rs"

run_selftest "check_base_duplicate_signatures.sh (unclassified duplicate Base method)" \
  "check_base_duplicate_signatures.sh" "selftest_duplicate_signature_10185(Any)" \
  inject_base_duplicate_signatures \
  "selftest_duplicate_signature_10185" "subset_julia_vm/src/julia/base/int.jl"

run_selftest "check_compile_expr_local_shadow_guard.sh (unguarded bare-name special-case)" \
  "check_compile_expr_local_shadow_guard.sh" "selftest10044" \
  inject_compile_expr_local_shadow_guard \
  "selftest10044" "subset_julia_vm_compile/src/compile/expr/mod.rs"

run_selftest "check_compile_expr_local_shadow_guard.sh (InternedStr projection grammar removed)" \
  "check_compile_expr_local_shadow_guard.sh" \
  "guard grammar conformance rejected accepted form 'interned-str projection'" \
  inject_compile_expr_local_shadow_guard_projection_removed \
  "SELFTEST11604-PROJECTION-REMOVED" "scripts/check_compile_expr_local_shadow_guard.sh"

run_selftest "check_specializer_callee_guard.sh (name-keyed arm before local-callee guard)" \
  "check_specializer_callee_guard.sh" "BEFORE the local-callee guard" \
  inject_specializer_callee_guard \
  "selftest10418" "subset_julia_vm_vm/src/vm/specialize/expr.rs"

run_selftest "check_type_application_matrix.sh (unrepresented type-application opcode)" \
  "check_type_application_matrix.sh" "not represented in the matrix" \
  inject_type_application_matrix \
  "ApplyTypeSelftestBogus10556" "subset_julia_vm_bytecode/src/instr.rs"

run_selftest "check_orphaned_rs_files.sh (unreferenced .rs file under crate src/)" \
  "check_orphaned_rs_files.sh" "selftest10739orphan.rs" \
  inject_orphaned_rs_files \
  "selftest10739orphan" "subset_julia_vm/src/selftest10739orphan.rs"

run_selftest "check_lambda_context_routing.sh (narrow predicate outside the authority)" \
  "check_lambda_context_routing.sh" "lambda_context_routing R1 violation" \
  inject_lambda_context_routing \
  "selftest10936" "subset_julia_vm_lowering/src/lowering/stmt/mod.rs"

run_selftest "check_lambda_context_routing.sh (laundering wrapper inside the authority)" \
  "check_lambda_context_routing.sh" "lambda_context_routing R3 violation" \
  inject_lambda_context_routing_wrapper_shim \
  "selftest11179" "subset_julia_vm_lowering/src/lowering/mod.rs"

run_selftest "check_lambda_context_routing.sh (post-hoc struct-new watermark stamp, Issue #11211)" \
  "check_lambda_context_routing.sh" "lambda_context_routing R4 violation" \
  inject_lambda_context_posthoc_struct_new_stamp \
  "SELFTEST11211-POSTHOC" "subset_julia_vm_lowering/src/lowering/mod.rs"

run_selftest "check_constructor_identity_authority.sh (per-signature side boolean)" \
  "check_constructor_identity_authority.sh" "forbidden side boolean in MethodSig" \
  inject_constructor_identity_side_boolean \
  "SelftestConstructorIdentitySideBoolean11043" \
  "subset_julia_vm_bytecode/src/method_table.rs"

run_selftest "check_constructor_identity_authority.sh (disabled table authority query)" \
  "check_constructor_identity_authority.sh" \
  "must return only the direct constructor_self_families membership query" \
  inject_constructor_identity_disabled_query \
  "SelftestConstructorIdentityDisabledQuery11043" \
  "subset_julia_vm_bytecode/src/method_table.rs"

run_selftest "check_constructor_identity_authority.sh (disabled selector rejection)" \
  "check_constructor_identity_authority.sh" \
  "inner-constructor query as the leading rejection disjunct" \
  inject_constructor_identity_disabled_selector \
  "SelftestConstructorIdentityDisabledSelector11043" \
  "subset_julia_vm_compile/src/compile/expr/call/constructors.rs"

run_selftest "check_constructor_return_identity.sh (same-base first-match, Issue #11436)" \
  "check_constructor_return_identity.sh" \
  "get_struct_type_id contains forbidden family first-winner selector 'iter().find'" \
  inject_constructor_return_family_first_match \
  "SelftestConstructorReturnFamilyFirstMatch11436" \
  "subset_julia_vm_compile/src/compile/context.rs"

run_selftest "check_constructor_return_identity.sh (CoreCompiler family first-match, Issue #11436)" \
  "check_constructor_return_identity.sh" \
  "julia_type_to_value_type_with_ctx contains forbidden family first-winner selector 'struct-table borrowed iteration'" \
  inject_constructor_return_core_family_first_match \
  "SelftestConstructorReturnCoreFamilyFirstMatch11436" \
  "subset_julia_vm_compile/src/compile/core_compiler.rs"

run_selftest "check_constructor_return_identity.sh (typed-array bare-family lookup, Issue #11436)" \
  "check_constructor_return_identity.sh" \
  "heap_julia_type_array_element_type_resolved lost exact-or-Any evidence" \
  inject_constructor_return_typed_array_family_lookup \
  "SelftestConstructorReturnTypedArrayFamilyLookup11436" \
  "subset_julia_vm_compile/src/compile/expr/builtin_array.rs"

run_selftest "check_constructor_return_identity.sh (instantiated TypeVar sharpening, Issue #11436)" \
  "check_constructor_return_identity.sh" \
  "infer_value_instantiated_ctor lost exact-or-Any evidence" \
  inject_constructor_return_instantiated_typevar \
  "SelftestConstructorReturnInstantiatedTypevar11436" \
  "subset_julia_vm_compile/src/compile/expr/infer/expr_tfuncs.rs"

run_selftest "check_constructor_return_identity.sh (unresolved owner inference, Issue #11510)" \
  "check_constructor_return_identity.sh" \
  "infer_expr_type lost exact-or-Any evidence" \
  inject_constructor_return_unresolved_owner_inference \
  "SelftestConstructorReturnUnresolvedOwnerInference11510" \
  "subset_julia_vm_compile/src/compile/expr/infer/mod.rs"

run_selftest "check_math_router_exact_or_any.sh (argument-blind tfunc fabricates concrete, Issue #11486)" \
  "check_math_router_exact_or_any.sh" \
  "tfunc_complex_contextual must return LatticeType::Top" \
  inject_math_router_fabricates_concrete \
  "LatticeType::Concrete(ConcreteType::Struct" \
  "subset_julia_vm_compile/src/compile/tfuncs/complex_ops.rs"

run_selftest "check_struct_registry_first_match.sh (order-derived same-base scan, Issue #11436)" \
  "check_struct_registry_first_match.sh" \
  "hash-backed struct registry first-winner scan inventory drifted" \
  inject_struct_registry_first_match_scan \
  "selftest11436_first_match_scan" \
  "subset_julia_vm_compile/src/compile/expr/call/constructors.rs"

run_selftest "check_constructor_owner_resolution.sh (direct leaf fallback, Issue #11172)" \
  "check_constructor_owner_resolution.sh" \
  "try_compile_struct_table_constructor_call has 2 short_constructor_name calls" \
  inject_constructor_owner_short_fallback \
  "selftest11172_owner_losing_fallback" \
  "subset_julia_vm_compile/src/compile/expr/call/constructors.rs"

run_selftest "check_constructor_owner_resolution.sh (runtime callee probe, Issue #11713)" \
  "check_constructor_owner_resolution.sh" \
  "compile_runtime_datatype_value_call lost ordered owner-resolution evidence 'runtime_nominal_binding_name(&type_name)'" \
  inject_constructor_runtime_callee_probe_removed \
  "SelftestRuntimeCalleeProbe11713" \
  "subset_julia_vm_compile/src/compile/expr/call/module_call.rs"

run_selftest "check_constructor_owner_resolution.sh (parametric runtime callee probe, Issue #11713)" \
  "check_constructor_owner_resolution.sh" \
  "compile_runtime_datatype_value_call lost ordered owner-resolution evidence 'runtime_nominal_binding_name(&base_name)'" \
  inject_constructor_parametric_runtime_probe_removed \
  "SelftestParametricRuntimeProbe11713" \
  "subset_julia_vm_compile/src/compile/expr/call/module_call.rs"

run_selftest "check_constructor_owner_resolution.sh (dynamic parametric runtime callee probe, Issue #11713)" \
  "check_constructor_owner_resolution.sh" \
  "compile_dynamic_parametric_struct lost required owner-resolution evidence 'runtime_nominal_binding_name(&qualified_base_name)'" \
  inject_constructor_dynamic_parametric_runtime_probe_removed \
  "SelftestDynamicParametricRuntimeProbe11713" \
  "subset_julia_vm_compile/src/compile/expr/call/dynamic.rs"

run_selftest "check_constructor_owner_resolution.sh (runtime probe Base-origin guard, Issue #11716)" \
  "check_constructor_owner_resolution.sh" \
  "runtime_nominal_binding_name lost required owner-resolution evidence 'if self.type_is_base_origin(type_name)'" \
  inject_constructor_runtime_probe_base_origin_guard_removed \
  "SelftestRuntimeProbeBaseOrigin11716" \
  "subset_julia_vm_compile/src/compile/expr/call/dynamic.rs"

run_selftest "check_constructor_owner_resolution.sh (runtime probe current-input guard, Issue #11716)" \
  "check_constructor_owner_resolution.sh" \
  "runtime_nominal_binding_name lost required owner-resolution evidence 'if !is_current_input'" \
  inject_constructor_runtime_probe_current_input_guard_removed \
  "SelftestRuntimeProbeCurrentInput11716" \
  "subset_julia_vm_compile/src/compile/expr/call/dynamic.rs"

run_selftest "check_constructor_owner_resolution.sh (runtime lexical owner preference, Issue #11733)" \
  "check_constructor_owner_resolution.sh" \
  "runtime_nominal_binding_name lost required owner-resolution evidence 'lexical_qualified.filter(|qualified|'" \
  inject_constructor_runtime_lexical_owner_preference_removed \
  "SelftestRuntimeLexicalOwner11733" \
  "subset_julia_vm_compile/src/compile/expr/call/dynamic.rs"

run_selftest "check_constructor_owner_resolution.sh (runtime bare constructor route, Issue #11733)" \
  "check_constructor_owner_resolution.sh" \
  "compile_call lost ordered owner-resolution evidence 'runtime_nominal_binding_name(function)'" \
  inject_constructor_runtime_bare_route_removed \
  "SelftestRuntimeBareRoute11733" \
  "subset_julia_vm_compile/src/compile/expr/call/mod.rs"

run_selftest "check_constructor_owner_resolution.sh (runtime enum lexical probe, Issue #11733)" \
  "check_constructor_owner_resolution.sh" \
  "try_compile_enum_call lost ordered owner-resolution evidence 'runtime_nominal_binding_name(function)'" \
  inject_constructor_runtime_enum_probe_removed \
  "SelftestRuntimeEnumProbe11733" \
  "subset_julia_vm_compile/src/compile/expr/call/mod.rs"

run_selftest "check_constructor_owner_resolution.sh (runtime enum member binding, Issue #11733)" \
  "check_constructor_owner_resolution.sh" \
  "collect_module_body_binding_names lost required owner-resolution evidence 'RuntimeNominalDef::Enum(enum_def)'" \
  inject_constructor_runtime_enum_member_binding_removed \
  "SelftestRuntimeEnumMember11733" \
  "subset_julia_vm_compile/src/compile/collect.rs"

run_selftest "check_constructor_owner_resolution.sh (static registry bare-route exclusion, Issues #11716/#11684)" \
  "check_constructor_owner_resolution.sh" \
  "compile_call lost required owner-resolution evidence '!self.shared_ctx.struct_table.contains_key(&static_binding)'" \
  inject_constructor_static_registry_bare_route_guard_removed \
  "SelftestStaticRegistryBareRoute11716" \
  "subset_julia_vm_compile/src/compile/expr/call/mod.rs"

run_selftest "check_constructor_owner_resolution.sh (package type chronology provenance, Issue #11716)" \
  "check_constructor_owner_resolution.sh" \
  "package type chronology provenance lost required owner-resolution evidence 'module_value.mark_as_package_origin()'" \
  inject_constructor_package_type_provenance_removed \
  "SelftestPackageTypeProvenance11716" \
  "subset_julia_vm/src/loader.rs"

run_selftest "check_constructor_owner_resolution.sh (Base module nominal provenance, Issue #11737)" \
  "check_constructor_owner_resolution.sh" \
  "current-source nominal origin boundary lost required owner-resolution evidence 'self.is_base_origin = true'" \
  inject_constructor_base_module_provenance_removed \
  "SelftestBaseModuleProvenance11737" \
  "subset_julia_vm_types/src/ir/core.rs"

run_selftest "check_constructor_owner_resolution.sh (current-main nominal boundary, Issue #11737)" \
  "check_constructor_owner_resolution.sh" \
  "current-main runtime nominal origin boundary lost required owner-resolution evidence '.position(is_base_user_main_boundary)'" \
  inject_constructor_current_main_nominal_boundary_removed \
  "SelftestCurrentMainNominalBoundary11737" \
  "subset_julia_vm_compile/src/compile/pipeline_ctx.rs"

run_selftest "check_constructor_owner_resolution.sh (inherited-module nominal filter, Issue #11737)" \
  "check_constructor_owner_resolution.sh" \
  "inherited-module runtime nominal origin boundary lost required owner-resolution evidence 'module.is_base_origin || module.is_package_origin'" \
  inject_constructor_inherited_module_nominal_filter_removed \
  "SelftestInheritedModuleNominalFilter11737" \
  "subset_julia_vm_compile/src/compile/collect.rs"

run_selftest "check_constructor_owner_resolution.sh (current-input nominal provenance, Issue #11716)" \
  "check_constructor_owner_resolution.sh" \
  "REPLSession current-input nominal provenance lost required owner-resolution evidence '&current_input_type_names'" \
  inject_constructor_current_input_type_provenance_removed \
  "SelftestCurrentInputTypeProvenance11716" \
  "subset_julia_vm/src/repl/session.rs"

run_selftest "check_constructor_owner_resolution.sh (synthetic restoration span guard, Issue #11716)" \
  "check_constructor_owner_resolution.sh" \
  "compile_call lost required owner-resolution evidence 'call_span.start != call_span.end'" \
  inject_constructor_synthetic_span_guard_removed \
  "SelftestSyntheticSpanGuard11716" \
  "subset_julia_vm_compile/src/compile/expr/call/mod.rs"

run_selftest "check_constructor_owner_resolution.sh (inner parametric runtime callee probe, Issue #11713)" \
  "check_constructor_owner_resolution.sh" \
  "compile_dynamic_parametric_constructor_method_call lost ordered owner-resolution evidence 'runtime_nominal_binding_name(&qualified_base_name)'" \
  inject_constructor_inner_parametric_runtime_probe_removed \
  "SelftestInnerParametricRuntimeProbe11713" \
  "subset_julia_vm_compile/src/compile/expr/call/dynamic.rs"

run_selftest "check_constructor_owner_resolution.sh (splat parametric runtime callee probe, Issue #11713)" \
  "check_constructor_owner_resolution.sh" \
  "try_compile_splat_parametric_constructor_call lost ordered owner-resolution evidence 'runtime_nominal_binding_name(&resolved_base_name)'" \
  inject_constructor_splat_parametric_runtime_probe_removed \
  "SelftestSplatParametricRuntimeProbe11713" \
  "subset_julia_vm_compile/src/compile/expr/call/constructors.rs"

run_selftest "check_constructor_owner_resolution.sh (static callee materialization, Issue #11716)" \
  "check_constructor_owner_resolution.sh" \
  "compile_runtime_datatype_value_call lost required owner-resolution evidence 'Instr::PushDataType(type_name)'" \
  inject_constructor_static_callee_materialization_removed \
  "SelftestStaticCallee11716" \
  "subset_julia_vm_compile/src/compile/expr/call/module_call.rs"

run_selftest "check_constructor_owner_resolution.sh (static forward guard, Issue #11720)" \
  "check_constructor_owner_resolution.sh" \
  "compile_resolved_module_call lost ordered owner-resolution evidence 'Instr::ThrowUndefVarError(constructor_base)'" \
  inject_constructor_static_forward_guard_removed \
  "SelftestStaticForward11720" \
  "subset_julia_vm_compile/src/compile/expr/call/module_call.rs"

run_selftest "check_constructor_owner_resolution.sh (runtime qualified guard, Issue #11172)" \
  "check_constructor_owner_resolution.sh" \
  "constructor_type_heads_match lost required owner-resolution evidence 'if left.contains() || right.contains()'" \
  inject_constructor_runtime_owner_guard_disabled \
  "SelftestConstructorRuntimeOwnerGuard11172" \
  "subset_julia_vm_vm/src/vm/exec/call_function_variable.rs"

run_selftest "check_constructor_owner_resolution.sh (Base parametric registry, Issue #11369)" \
  "check_constructor_owner_resolution.sh" \
  "build_struct_tables has 1 occurrences of 'module_path.is_none() && stored_def.is_base_origin'; expected 2" \
  inject_constructor_base_parametric_registry_removed \
  "SelftestConstructorBaseParametricRegistry11369" \
  "subset_julia_vm_compile/src/compile/pipeline_ctx.rs"

run_selftest "check_constructor_owner_resolution.sh (Base collection owner, Issue #11369)" \
  "check_constructor_owner_resolution.sh" \
  "try_compile_explicit_public_dict_constructor lost required owner-resolution evidence 'resolve_instantiation_with_type_expr(\"Base.Dict\", &type_args)'" \
  inject_constructor_base_collection_owner_erased \
  "SelftestConstructorBaseCollectionOwner11369" \
  "subset_julia_vm_compile/src/compile/expr/call/mod.rs"

run_selftest "check_constructor_owner_resolution.sh (Base parametric lowering, Issue #11369)" \
  "check_constructor_owner_resolution.sh" \
  "split_base_parametric_call_target lost required owner-resolution evidence 'module != \"Base\"'" \
  inject_constructor_base_parametric_lowering_erased \
  "SelftestConstructorBaseParametricLowering11369" \
  "subset_julia_vm_lowering/src/lowering/expr/call.rs"

run_selftest "check_constructor_owner_resolution.sh (Base nested field owner, Issue #11369)" \
  "check_constructor_owner_resolution.sh" \
  "substitute_field_type lost required owner-resolution evidence 'owned_base.is_some() || self.parametric_structs.contains_key(resolved_base)'" \
  inject_constructor_base_nested_field_owner_erased \
  "SelftestConstructorBaseNestedFieldOwner11369" \
  "subset_julia_vm_compile/src/compile/context.rs"

run_selftest "check_constructor_owner_resolution.sh (Base concrete identity, Issue #11369)" \
  "check_constructor_owner_resolution.sh" \
  "resolve_instantiation_with_type_expr lost required owner-resolution evidence 'base.to_string()'" \
  inject_constructor_base_concrete_identity_qualified \
  "SelftestConstructorBaseConcreteIdentity11369" \
  "subset_julia_vm_compile/src/compile/context.rs"

run_selftest "check_constructor_owner_resolution.sh (Base type-expression owner, Issue #11369)" \
  "check_constructor_owner_resolution.sh" \
  "emit_type_expr_value_for_array_alloc lost required owner-resolution evidence 'if explicit_base_owner'" \
  inject_constructor_base_type_expr_owner_erased \
  "SelftestConstructorBaseTypeExprOwner11369" \
  "subset_julia_vm_compile/src/compile/expr/collection.rs"

run_selftest "check_constructor_owner_resolution.sh (compile splat owner guard, Issue #11371)" \
  "check_constructor_owner_resolution.sh" \
  "compile_call lost required owner-resolution evidence 'if has_splat && !self.locals.contains_key(function) && !self.captured_vars.contains(function)'" \
  inject_constructor_compile_splat_owner_guard_disabled \
  "SelftestConstructorCompileSplatOwnerGuard11371" \
  "subset_julia_vm_compile/src/compile/expr/call/mod.rs"

run_selftest "check_constructor_owner_resolution.sh (parametric callee capture producer, Issue #11373)" \
  "check_constructor_owner_resolution.sh" \
  "analyze_expr_free_vars lost required owner-resolution evidence 'parse_parametric_call(function)'" \
  inject_constructor_parametric_callee_capture_disabled \
  "SelftestConstructorParametricCalleeCapture11373" \
  "subset_julia_vm_types/src/ir/free_vars.rs"

run_selftest "check_constructor_owner_resolution.sh (dynamic parametric evaluation order, Issue #11375)" \
  "check_constructor_owner_resolution.sh" \
  "compile_dynamic_parametric_struct must keep 'Instr::StoreAny(type_arg_temp.clone())' before 'for arg in args'" \
  inject_constructor_dynamic_parametric_order_reversed \
  "SelftestConstructorDynamicParametricOrder11375" \
  "subset_julia_vm_compile/src/compile/expr/call/dynamic.rs"

run_selftest "check_constructor_owner_resolution.sh (runtime splat default fallback, Issue #11371)" \
  "check_constructor_owner_resolution.sh" \
  "execute_call_function_variable has 3 calls to 'try_construct_default_datatype(&func_name, &expanded_args'; expected 4" \
  inject_constructor_runtime_splat_default_fallback_removed \
  "SelftestConstructorRuntimeSplatDefaultFallback11371" \
  "subset_julia_vm_vm/src/vm/exec/call_function_variable.rs"

run_selftest "check_binding_provenance_authority.sh (exhaustive LocalDeclKind consumer, Issue #11317)" \
  "check_binding_provenance_authority.sh" \
  "collect_stmt lost required provenance evidence 'LocalDeclKind::CompilerEnclosing'" \
  inject_binding_provenance_consumer_wildcard \
  "SelftestBindingProvenanceWildcard11317" \
  "subset_julia_vm_lowering/src/lowering/scope_bindings.rs"

run_selftest "check_binding_provenance_authority.sh (AoT consumer cannot ignore provenance, Issue #11317)" \
  "check_binding_provenance_authority.sh" \
  "convert_stmt_expanded lost required provenance evidence 'kind'" \
  inject_binding_provenance_aot_consumer_ignored \
  "SelftestBindingProvenanceAotIgnored11317" \
  "subset_julia_vm/src/aot/analyze/ir_converter/stmt.rs"

run_selftest "check_binding_provenance_authority.sh (unclassified if-let consumer, Issue #11317)" \
  "check_binding_provenance_authority.sh" \
  "selftest_unclassified_local_decl_consumer_11317 consumes LocalDecl.var without LocalDecl.kind" \
  inject_binding_provenance_unclassified_if_let_consumer \
  "SelftestBindingProvenanceUnclassifiedConsumer11317" \
  "subset_julia_vm_compile/src/compile/constants.rs"

run_selftest "check_binding_provenance_authority.sh (exact key-authority helper, Issue #11317)" \
  "check_binding_provenance_authority.sh" \
  "emit_load_declared_global body drifted from the exact key-authority expression" \
  inject_binding_provenance_authority_helper_corrupted \
  "SelftestBindingProvenanceAuthorityHelperCorrupted11317" \
  "subset_julia_vm_compile/src/compile/core_compiler.rs"

run_selftest "check_binding_provenance_authority.sh (owner-qualified declared global, Issue #11317)" \
  "check_binding_provenance_authority.sh" \
  "load_local declared-global branch drifted from its exact emit_load_declared_global authority path" \
  inject_binding_provenance_bare_global_key \
  "SelftestBindingProvenanceBareGlobalKey11317" \
  "subset_julia_vm_compile/src/compile/expr/mod.rs"

run_selftest "check_base_exports_subset.sh (non-upstream Base export, Issue #11298)" \
  "check_base_exports_subset.sh" \
  "identifiers absent from upstream Julia: Base" \
  inject_base_exports_non_upstream \
  "SelftestBaseExport11298" \
  "subset_julia_vm/src/julia/base/exports.jl"

run_selftest "check_source_position_chronology.sh (typed chronology API, Issue #11100)" \
  "check_source_position_chronology.sh" \
  "expand_for_signature must accept 'use_position: SourcePosition' instead of a raw offset" \
  inject_source_position_api_raw_usize \
  "SelftestSourcePositionApi11100" \
  "subset_julia_vm_lowering/src/lowering/type_alias.rs"

run_selftest "check_source_position_chronology.sh (raw offset comparison, Issue #11100)" \
  "check_source_position_chronology.sh" \
  "raw source-order offset comparison(s) bypass SourcePosition" \
  inject_source_position_raw_offset_compare \
  "SelftestRawSourceOrder11100" \
  "subset_julia_vm_lowering/src/lowering/type_alias.rs"

run_selftest "check_source_position_chronology.sh (raw offset cmp method, Issue #11100)" \
  "check_source_position_chronology.sh" \
  "raw source-order offset comparison(s) bypass SourcePosition" \
  inject_source_position_raw_offset_cmp \
  "SelftestRawSourceOrderCmp11100" \
  "subset_julia_vm_lowering/src/lowering/type_alias.rs"

run_selftest "check_python_audit_compatibility.sh (Python 3.10-only syntax)" \
  "check_python_audit_compatibility.sh" "syntax requires Python newer than 3.9" \
  inject_python_audit_newer_syntax \
  "SelftestPythonNewHelperDiscovery11102" "scripts/python_discovery_selftest.py"

run_selftest "check_python_audit_compatibility.sh (option-prefix discovery bypass)" \
  "check_python_audit_compatibility.sh" "options before an external helper are forbidden" \
  inject_python_audit_option_bypass \
  "SelftestPythonOptionBypass11102" "scripts/check_python_option_bypass_selftest.sh"

run_selftest "check_python_audit_compatibility.sh (Python 3.11 stdlib import)" \
  "check_python_audit_compatibility.sh" "import 'tomllib' is not in the Python 3.9 verified import set" \
  inject_python_audit_newer_stdlib \
  "SelftestPythonNewerStdlib11102" "scripts/unsafe_inventory.py"

run_selftest "check_python_audit_compatibility.sh (eager PEP 604 annotation)" \
  "check_python_audit_compatibility.sh" "evaluated PEP 604 annotation needs" \
  inject_python_audit_eager_union \
  "SelftestPythonEagerUnion11102" "scripts/check_orphaned_rs_files.py"

run_selftest "check_test_aot_vm_aot_lane.sh (vm_aot lane invocation removed from test_aot.sh, Issue #10815)" \
  "check_test_aot_vm_aot_lane.sh" "no longer invokes 'metamorphic_equivalence.sh ... --lane vm_aot'" \
  inject_test_aot_vm_aot_lane_missing \
  "SELFTEST10815-REMOVED-VM-AOT-LANE" "scripts/test_aot.sh"

run_selftest "check_test_aot_vm_aot_lane.sh (vm_aot corpus shrunk below the Issue #10815 floor)" \
  "check_test_aot_vm_aot_lane.sh" "below the Issue #10815 floor" \
  inject_vm_aot_corpus_shrunk \
  "SELFTEST10815-SHRUNK-VM-AOT-CORPUS" "tests/equivalence/vm_aot.tsv"

run_selftest "check_test_aot_vm_aot_lane.sh (fixed AoT binary path restored, Issue #11598)" \
  "check_test_aot_vm_aot_lane.sh" "AoT harness/nightly contract regression tests failed" \
  inject_test_aot_fixed_binary_path \
  "SELFTEST11598-FIXED-TARGET" "scripts/test_aot.sh"

run_selftest "check_test_aot_vm_aot_lane.sh (direct helper binary reassigned, Issue #11598)" \
  "check_test_aot_vm_aot_lane.sh" "AoT harness/nightly contract regression tests failed" \
  inject_aot_vm_fixed_binary_reassignment \
  "SELFTEST11598-DIRECT-REASSIGN" "scripts/aot_vm_differential.sh"

run_selftest "check_test_aot_vm_aot_lane.sh (Cargo target reset after binary derivation, Issue #11598)" \
  "check_test_aot_vm_aot_lane.sh" "AoT harness/nightly contract regression tests failed" \
  inject_test_aot_late_target_reset \
  "SELFTEST11598-LATE-TARGET-RESET" "scripts/test_aot.sh"

run_selftest "check_test_aot_vm_aot_lane.sh (bare fixed-path invocation bypasses variable, Issue #11598)" \
  "check_test_aot_vm_aot_lane.sh" "AoT harness/nightly contract regression tests failed" \
  inject_aot_vm_bare_fixed_binary_invocation \
  "SELFTEST11598-BARE-FIXED-INVOKE" "scripts/aot_vm_differential.sh"

run_selftest "check_test_aot_vm_aot_lane.sh (command-local Cargo target reset, Issue #11598)" \
  "check_test_aot_vm_aot_lane.sh" "AoT harness/nightly contract regression tests failed" \
  inject_test_aot_command_local_target_reset \
  "SELFTEST11598-COMMAND-TARGET-RESET" "scripts/test_aot.sh"

run_selftest "check_test_aot_vm_aot_lane.sh (target-dir invocation bypasses explicit override, Issue #11598)" \
  "check_test_aot_vm_aot_lane.sh" "AoT harness/nightly contract regression tests failed" \
  inject_aot_vm_target_dir_invocation \
  "SELFTEST11598-TARGET-DIR-INVOKE" "scripts/aot_vm_differential.sh"

run_selftest "check_test_aot_vm_aot_lane.sh (shell-hidden Cargo target reset, Issue #11598)" \
  "check_test_aot_vm_aot_lane.sh" "AoT harness/nightly contract regression tests failed" \
  inject_test_aot_shell_hidden_target_reset \
  "SELFTEST11598-SHELL-HIDDEN-TARGET" "scripts/test_aot.sh"

run_selftest "check_test_aot_vm_aot_lane.sh (inert variable use masks executable bypass, Issue #11598)" \
  "check_test_aot_vm_aot_lane.sh" "AoT harness/nightly contract regression tests failed" \
  inject_aot_vm_inert_variable_compensation \
  "SELFTEST11598-INERT-VARIABLE-USE" "scripts/aot_vm_differential.sh"

run_selftest "check_test_aot_vm_aot_lane.sh (nightly scope category removed, Issue #11693)" \
  "check_test_aot_vm_aot_lane.sh" "AoT harness/nightly contract regression tests failed" \
  inject_nightly_fixture_scope_removed \
  "SELFTEST11693-REMOVED-SCOPE" ".github/workflows/nightly-gates.yml"

run_selftest "check_aot_gate_selection.sh (shared inference root removed, Issue #10866)" \
  "check_aot_gate_selection.sh" "shared inference-core changes no longer select the AoT gate" \
  inject_aot_gate_shared_inference_removed \
  "SELFTEST10866-REMOVED-INFERENCE-CORE" ".github/aot-gate-paths.txt"

run_selftest "check_aot_gate_selection.sh (legacy entry point removed, Issue #10866)" \
  "check_aot_gate_selection.sh" "legacy AoT entry-point changes no longer select the AoT gate" \
  inject_aot_gate_legacy_entrypoint_removed \
  "SELFTEST10866-REMOVED-LEGACY-ENTRYPOINT" ".github/aot-gate-paths.txt"

run_selftest "check_aot_gate_selection.sh (ci workflow delegation removed, Issue #10866)" \
  "check_aot_gate_selection.sh" ".github/workflows/ci.yml must delegate AoT path selection" \
  inject_aot_gate_ci_delegation_removed \
  "SELFTEST10866-CI-DISCONNECTED" ".github/workflows/ci.yml"

run_selftest "check_aot_gate_selection.sh (pr-fast consumer disconnected, Issue #10866)" \
  "check_aot_gate_selection.sh" "must gate the AoT job on the changes job's aot output exactly once" \
  inject_aot_gate_pr_consumer_disconnected \
  "SELFTEST10866-PR-CONSUMER-DISCONNECTED" ".github/workflows/pr-fast.yml"

run_selftest "check_rust_toolchain_contract.sh (AoT owner weakened to default lane, Issue #11253)" \
  "check_rust_toolchain_contract.sh" "test_aot.sh must invoke the registered 'aot' Clippy lane" \
  inject_test_aot_clippy_lane_weakened \
  "SELFTEST11253" "scripts/test_aot.sh"

run_selftest "check_rust_toolchain_contract.sh (new workspace member omits MSRV, Issue #11253)" \
  "check_rust_toolchain_contract.sh" \
  "selftest_missing_msrv_11253/Cargo.toml must inherit workspace.package.rust-version" \
  inject_workspace_member_without_msrv \
  "SELFTEST11253-MISSING-MSRV" "selftest_missing_msrv_11253/Cargo.toml"

run_selftest "check_rust_toolchain_contract.sh (CI lint job stops moving with stable, Issue #11253)" \
  "check_rust_toolchain_contract.sh" "CI must override the checked-in reference pin" \
  inject_ci_lint_job_not_current_stable \
  "SELFTEST11253-NOT-CURRENT-STABLE" ".github/workflows/ci.yml"

if [ "$REGISTRATION_ONLY" -eq 0 ]; then
  run_build_locked_contract_matrix
fi

completeness_check
anchor_policy_check
if [ "$LIST_TARGETS" -eq 1 ]; then
  printf 'target_path\taudit\tcontrol\n'
  printf '%b' "$TARGET_ROWS" | LC_ALL=C sort -u
  if [ "$overall_fail" -ne 0 ]; then
    exit 1
  fi
  exit 0
fi
if [ "$REGISTRATION_ONLY" -eq 1 ]; then
  if [ "$overall_fail" -ne 0 ]; then
    log "RESULT: FAIL — audit negative-self-test registration is incomplete."
    exit 1
  fi
  log "RESULT: OK — audit negative-self-test registration is complete."
  exit 0
fi

if [ "$FILTER_ACTIVE" -eq 1 ] && [ "$SELECTED_COUNT" -eq 0 ]; then
  if [ "$EXPLICIT_TARGET" -eq 1 ]; then
    bad "target selection: no negative control is registered for the requested path(s)"
  else
    pass "target selection: changed paths do not own semantic negative controls"
  fi
fi
silent_exit_lint

log ""
if [ "$overall_fail" -ne 0 ]; then
  log "RESULT: FAIL — an audit did not detect its injected violation, an audit is"
  log "        unaccounted for, or an audit can fail silently. A broken safety net"
  log "        leaves every later change unguarded. See docs/vm/CODE_AUDITS.md"
  log "        (Issues #9129 / #9388)."
  exit 1
fi
if [ "$FILTER_ACTIVE" -eq 1 ]; then
  log "RESULT: OK — $SELECTED_COUNT target-selected negative control(s) passed."
  exit 0
fi
log "RESULT: OK — every covered audit detects its injected violation with a stated,"
log "        injection-specific reason, every audit is covered or annotated, and no"
log "        audit script fails silently."
exit 0
