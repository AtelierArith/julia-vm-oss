# Full-Compile Reached Method Recovery Design

Issue: #11742
Date: 2026-07-19

## Problem

A fresh `REPLSession` can compile a conditional method and execute its
`DefineEvalFunction` marker before a later uncaught error. The VM therefore has
an exact reached activation, but the host drops it because
`full_compile_runtime_nominal_recovery_plan` returns `None` whenever the same
input has no runtime-nominal templates. Existing method recovery tests use a
warmed live-delta session; the conditional method test that exercises a fresh
full compile also defines a runtime nominal, masking the gate.

## Chosen Design

Rename the helper to `full_compile_definition_recovery_plan`. Admit a recovery
plan when the current input has a Julia-visible source method or the compiled
main has a runtime-nominal template. Compiler-generated constructor/helper
activations alone do not opt an otherwise normal input into recovery
validation. The existing typed prefix validator remains the sole authority for
whether a reached method can be committed; no counts, names, or source offsets
are inferred in the session layer.

Alternatives rejected:

- adding a dummy nominal template would corrupt the activation model;
- warming the session in the test would avoid the broken full-compile path;
- special-casing conditional methods would duplicate the generic activation
  mechanism already used for structs, abstract types, primitive types, and
  enums.

## Behavior and Error Handling

On a catchable error, a fresh full compile with at least one source method
builds the same `LiveErrorRecoveryPlan` used by runtime-nominal inputs. The VM
validates the exact observed prefix. Reached methods are retained in the
compiler/session mirror; source-later and untaken methods remain dormant. An
input with neither source methods nor runtime-nominal templates keeps the
existing definition-free error recovery path, even if lowering emitted helper
markers for an ordinary struct or closure.

Any activation mismatch continues to fail closed and discard the live VM. The
change does not relax `VmError::is_catchable`, world-age validation, registry
length checks, or source-order validation.

For a successful method-only full compile, the recovery plan is intentionally
ignored: the ordinary `store_definitions` path commits all source methods.
Success-prefix validation remains active for live deltas and for full compiles
that must adopt runtime-nominal activations into the compiler snapshot.

## Testing

Add one fresh-session regression that:

1. executes a conditional method marker;
2. throws before a second method marker;
3. verifies the first method in a later eval;
4. executes a module barrier to force full-rebuild replay;
5. verifies the first method again and the second method remains undefined.

Run the existing reached-method/live-delta regressions and the #11654 runtime
nominal recovery module to prove the generalized gate preserves both paths.

## Completion Criteria

- The fresh-session regression passes and its pre-fix failure is recorded.
- Existing live-delta and runtime-nominal recovery tests pass.
- Default clippy and the full release suite pass before guarded merge.
