# Callable Singleton Identity Prevention Design

Issue: #11703

## Problem

Runtime callables have two kinds of data that must not be confused:

- candidate function-table indices, which are relocatable execution references;
- `CallableSingletonIdentity`, which is the stable Julia-visible identity and
  includes owner and source-versus-lowering-helper provenance.

`FunctionValue` and `ClosureValue` carry both. Native generator callables may
carry only function-table indices until a rebuild makes a one-to-one relocation
impossible, at which point they must be converted to runtime callable values
without reconstructing identity from a display name.

The regression behind #11685 occurred when a generated helper and a
source-visible callable shared a spelling. Candidate indices and names were
insufficient to keep them distinct across a live append or fresh rebuild.

## Design

Add two mutually reinforcing prevention layers.

### Behavioral carrier matrix

Extend the existing consolidated VM callable-identity tests with a deterministic
matrix covering:

- `FunctionValue`;
- `ClosureValue`;
- `GeneratorCallable::FunctionIndex`;
- `GeneratorCallable::TupleSplatFunctionIndex`;
- both indices in `GeneratorCallable::FilteredFunctionIndex`.

Each carrier is persisted through a function-table relocation. The matrix uses
a same-spelled source callable as the only rebuilt candidate for an old lowering
helper, proving that remapping fails closed while preserving the helper singleton
identity. It also includes an owner-qualified callable that relocates normally,
proving that owner identity is not lost while indices move. Generator index
forms that cannot map one-to-one must become their corresponding runtime-value
forms containing `FunctionValue`s with the original identity.

Existing public REPL collision tests continue to cover dispatch, `typeof`, and
reflection. Existing comparison and deep-copy tests remain the behavioral sinks;
the source audit below pins their use of the stable authority.

### Source-only authority audit

Add `scripts/check_callable_singleton_identity.sh`, register it in the canonical
source-only audit registry, and document it in `docs/vm/CODE_AUDITS.md`.

The audit must fail closed when:

- a new `GeneratorCallable` function-index form appears without being added to
  the relocation matrix;
- `FunctionValue` or `ClosureValue` loses its private
  `CallableSingletonIdentity` carrier or authority accessor;
- callable construction, comparison, deep copy, runtime type keys, persisted
  replacement, or reflection stops routing through stable identity/provenance;
- generator fallback replacement stops using
  `persisted_function_value_for_index`.

The audit deliberately checks semantic anchors, not raw occurrence counts alone.
Its registered mutation self-test removes a required authority call and verifies
that the audit emits a specific failure.

## Verification

- Run the new carrier matrix and the existing #11685/#9784 callable tests.
- Run the audit directly and through its negative mutation self-test.
- Run all registered source-only audits, formatting, default Clippy, and the
  full release nextest suite before merge.
