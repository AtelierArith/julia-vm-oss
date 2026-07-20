# Typed-loop effect classification design

## Context

`TypedLoopOp` blocks bail by restarting the whole block in the generic
interpreter. Frame and array writes are transactional, but `RandF64` advances
the shared RNG immediately. The recognizer therefore rejects a block that
contains both a data-dependent bail point and an out-of-buffer effect.

Today those two facts are maintained by separate, non-exhaustive `matches!`
lists. A new enum variant compiles even when it is absent from both lists, so
the transactionality guard can silently become incomplete.

## Decision

Define a small `TypedLoopOpEffects` value with `bail_capable` and
`out_of_buffer_effect` booleans. Implement `TypedLoopOp::effects()` with one
exhaustive `match` over every variant and no wildcard arm. The recognizer will
derive both aggregate facts from that method and retain its current conservative
rule: reject exactly when any op is bail-capable and any op has an
out-of-buffer effect.

`RandF64` is the only current out-of-buffer effect. The existing
data-dependent bail set remains unchanged. In particular, `IndexStoreI64` and
`IndexStoreF64` remain bail-capable but not out-of-buffer effects because their
writes use the discardable `ArrayWriteOrigin` transaction buffer.

## Alternatives considered

- Two exhaustive methods would make additions fail to compile, but duplicate
  the enum classification surface and allow the two facts to drift apart.
- A macro-generated enum/metadata table would centralize the data, but it is a
  larger representation change than this prevention issue needs.
- Making all typed-loop effects transactional would remove the guard, but is
  the separate P1 design in Issue #10814 and would change runtime architecture.

## Verification

The classification unit test will assert every non-default category and the
safe defaults. The existing recognizer and differential RNG tests will prove
that accepted/rejected behavior is unchanged. Because `effects()` has no
wildcard arm, adding a `TypedLoopOp` variant without classifying it is a Rust
compile error, which is the required prevention mechanism.

No upstream Julia semantic investigation is needed: this is internal VM
transactionality metadata and intentionally preserves current Julia-visible
behavior.
