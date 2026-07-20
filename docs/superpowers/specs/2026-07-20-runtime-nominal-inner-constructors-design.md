# Runtime Nominal Inner Constructors Design

Issue: #11679

## Goal

A struct declaration reached inside top-level control flow must publish its
explicit inner constructors with the same semantics as an ordinary root struct
declaration. An unreached declaration publishes neither the type nor its
constructors.

## Chosen architecture

Reuse the existing compiler-owned constructor pipeline. During compilation,
retain the complete `StructDef`, register and compile its inner constructors by
the same logic used for root declarations, but keep their callable bodies
dormant and associate their activation with the runtime nominal site's stable
identity. `DefineRuntimeNominal` remains the source-order publication point: it
publishes the nominal registry entry and the associated constructor method group
in one VM world transition.

Do not parse or compile constructor source in the VM, and do not teach the raw
field constructor to emulate `new`. Those alternatives would duplicate Julia
constructor semantics and revive the runtime-eval/compiler split.

## Semantics

- A reached non-parametric runtime struct with explicit inner constructors uses
  those methods and suppresses the synthesized default field constructor.
- Constructor bodies preserve lexical owner, `new` allocation target, method
  signatures, and source-order visibility exactly as on the root path.
- A skipped runtime site leaves its type and constructor methods unavailable.
- Publication is atomic for recovery: a retained reached site carries both the
  nominal identity and its constructor activation; an unreached suffix carries
  neither.
- Existing runtime structs without inner constructors retain their current raw
  field-constructor behavior.
- Parametric runtime structs remain outside #11679 unless the shared root path
  makes them work without a new special case.

## Data flow

1. Lowering retains `StructDef.inner_constructors` in `RuntimeStructDefInfo`.
2. The compiler registers constructor methods through the established root
   constructor machinery and records their dormant activation group against the
   runtime nominal site.
3. The emitted runtime nominal marker identifies both the type metadata and the
   constructor activation group.
4. The VM preflights the nominal declaration, publishes the type, activates the
   constructor group in the same world, then records one reached definition
   activation for REPL recovery.
5. Cache serialization/restoration preserves the new association and receives a
   schema-version/fingerprint update if serialized metadata changes.

## Failure and recovery

All validation that can fail before publication remains mutation-free. If
constructor activation can fail after type publication, the implementation must
either preflight it or roll back both sides; it may not retain a type without its
explicit constructors. Catchable-error recovery and full rebuild must reproduce
the same reached pair.

## Tests

- Upstream fixture: the issue MWE returns field value `8`.
- Negative branch: an untaken declaration exposes neither the type nor ctor.
- Default suppression: an explicit inner constructor changes the result, proving
  the raw field constructor did not win.
- REPL session: a reached runtime struct and its constructor survive a later
  evaluation/rebuild with identity and dispatch intact.
- Existing no-inner-constructor runtime nominal matrix remains green.
- Relevant category/session tests, cache schema audit, clippy, full release
  suite, and AoT gate (if shared/AoT metadata is touched) pass before merge.

## Scope

This change fixes #11679 only. It introduces no package-name shortcuts, no new
per-issue test binary, and no workaround. Broader activation-seam prevention
remains owned by #11564.
