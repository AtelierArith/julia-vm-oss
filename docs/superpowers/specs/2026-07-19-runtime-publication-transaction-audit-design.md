# Runtime Publication Transaction Audit Design

Issue: #11740
Date: 2026-07-19

## Problem

REPL execution publishes several kinds of persistent state: methods, nominal
types, imports, module shells, enum members, and ordinary globals. These values
cross lowering, compilation, VM execution, catchable-error recovery, and later
full-rebuild replay. Issue #11654 showed that a definition can be present in one
surface while absent, source-later, or owned by a different module in another.
The existing definition-order inventory detects Core-IR vector merges, but it
does not require every persistent runtime artifact to document its success,
error-recovery, and replay policies.

The prevention must fail closed when a new persistent publication family is
added without an explicit transaction policy, and it must exercise the policy
through the public `REPLSession` behavior rather than testing only helper
implementation details.

## Goals

- Extend the existing definition-order authority instead of creating a second
  chronology inventory.
- Enumerate the persistent definition/publication families that participate in
  REPL success, catchable-error recovery, and full-rebuild replay.
- Require an explicit policy for all three phases for every inventoried family.
- Detect a missing inventory row with a registered negative audit self-test.
- Add a compact behavioral matrix covering methods, runtime nominals, imports,
  and module bodies across success, caught failure, uncaught failure, and a
  subsequent eval that may use full-rebuild replay.
- Preserve the existing VM, compiler, AoT, iOS, and WASM behavior. This is a
  prevention change, not a new runtime feature.

## Non-goals

- Unifying all publication data into one new Rust enum or transaction object.
- Replacing `REPLSession`'s existing live-append and full-rebuild algorithms.
- Auditing transient VM caches, output buffers, heap objects, or ordinary
  frame-zero value assignments that are not definition replay artifacts.
- Expanding AoT support for runtime nominal statements.

## Chosen Approach

Extend `docs/vm/DEFINITION_ORDER_MERGE_INVENTORY.tsv` and
`scripts/check_definition_order_merges.sh` with a second, explicit runtime-state
inventory kind. Keeping Core-IR chronology and runtime publication in one
authority makes reviewers answer both questions at the same boundary: “where
was this fragment ordered?” and “what happens to its published state after an
error?”

Two alternatives were rejected:

1. A separate runtime-publication TSV would be locally simpler, but it would
   duplicate ownership of the same REPL merge/recovery seams.
2. Behavioral tests alone would catch known cases but would not fail when a new
   persistent field or activation collection is introduced without coverage.

## Inventory Model

The TSV keeps its current columns and adds three policy columns:

- `success_policy`: how the artifact is committed after a successful eval.
- `error_policy`: how the exact reached prefix is retained or why the artifact
  is discarded after a catchable or uncaught error.
- `replay_policy`: how stored state is reconstructed in a later full compile.

Existing `cursor`, `raw`, and `reviewed` rows use `not_runtime_state` in these
columns. New `state` rows describe one durable publication family and use a
stable symbol that appears in the source owner. The initial required families
are:

| Family | Source evidence | Success | Error | Replay |
|---|---|---|---|---|
| methods | `definition_activations`, `repl_definition_activations` | store reached definitions | typed reached prefix | merge stored definitions |
| runtime nominals | `runtime_nominal_activations` | adopt VM activations | exact activation sites | rebuild inert definitions |
| imports | `usings` | store distinct imports | retain only reached import statements | splice imports in chronology |
| modules | `modules` / `RecoveredModuleReplay` | store completed module | inert reached shell | replay shell without failed body |

The machine tokens are fixed: success uses `store_reached`, `vm_observed`,
`store_distinct`, or `store_completed`; error recovery uses `typed_prefix`,
`exact_sites`, `reached_statements`, or `inert_shell`; replay uses
`merge_definitions`, `rebuild_inert`, `splice_chronology`, or
`replay_inert_shell`. Only `not_runtime_state` is accepted for all three columns
on non-`state` rows.

The audit treats blank policies and unknown policy tokens as errors. It also
scans the authoritative `ReachedReplDefinitionPrefix`, `REPLSession`, and
`RecoveredModuleReplay` declarations for the stable evidence symbols and
requires matching `state` rows. It also discovers every field ending in
`_activations` with a `Vec<...>` carrier in `repl/session.rs` and `vm/mod.rs`;
an added persistent activation collection therefore produces inventory drift
instead of silently bypassing review.

The scanner remains intentionally syntactic and Bash-3-compatible: the shell
wrapper invokes an embedded Python 3 program, masks comments and string
literals where necessary, and reports every missing, duplicate, or stale row in
one run. It does not attempt Rust type inference.

## Negative Self-test

Register a mutation in `scripts/check_audit_negative_selftest.sh` and
`docs/vm/AUDIT_SELFTEST_ANCHORS.tsv` that removes the runtime-nominal `state`
row from a temporary copy of the inventory. The expected diagnostic names both
the missing family and `runtime_nominal_activations`. This proves semantic
completeness rather than merely proving that malformed TSV syntax is rejected.

The audit remains registered through the existing
`definition_order_merges` row in `scripts/source_only_audits.tsv`; no new CI
workflow entry is required.

## Behavioral Matrix

Add one table-driven regression under the existing
`runtime_nominal_repl_tests_11654`/REPL session test surface. Each case creates a
fresh deterministic `REPLSession`, evaluates a setup input, then evaluates a
probe input that forces persisted state to be consumed.

The matrix covers:

- method: a reached method before a later uncaught error remains callable;
- nominal: a reached conditional type before a later uncaught error remains
  constructible with the correct owner;
- import: a reached selective import remains usable while an import after the
  failure remains undefined;
- module: declarations reached before a module-body failure remain available,
  while the failed statement and source-later declarations are not replayed;
- caught control: a locally caught error continues execution and commits the
  later reached publication;
- skipped control: a declaration in an untaken branch is never published.

Assertions check value, type/owner where observable, and `isdefined` negative
controls. The tests use real session compilation and execution with no mocks.
Existing focused tests remain; this matrix is the cross-family prevention gate.

## Error Handling

The source audit fails with actionable diagnostics for:

- missing or duplicate state-family rows;
- missing policy text;
- an unsupported policy token;
- source evidence that no longer exists;
- a newly discovered authoritative state field without an inventory row.

Behavioral regressions retain Julia-facing error behavior. The prevention code
does not catch or translate runtime errors; it observes which state survives
through the existing session API.

## Verification

1. Run the existing audit before edits to establish the green baseline.
2. Add the negative self-test first and observe it fail because the audit does
   not yet enforce state rows.
3. Implement the inventory schema and scanner, then observe the self-test pass.
4. Add the table-driven REPL regression and run it against upstream Julia where
   its syntax is supported, then against sjulia.
5. Run source-only audits, the full audit negative-self-test suite, Bash 3
   compatibility, the focused REPL test binary, default clippy, and the full
   release nextest suite before merge.

## Completion Criteria

- Every required publication family has exactly one reviewed policy row.
- Removing the runtime-nominal row fails with the registered diagnostic.
- The cross-family REPL matrix passes with all negative controls intact.
- `bash scripts/run_source_only_audits.sh` and
  `bash scripts/check_audit_negative_selftest.sh` pass.
- `timeout 1800 cargo nextest run --release` passes before guarded merge.
