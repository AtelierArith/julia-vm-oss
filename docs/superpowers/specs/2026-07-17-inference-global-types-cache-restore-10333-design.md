# Persist inference globals across cache restore (#10333)

## Problem

Fresh compilation records `RuntimeCompileContext::inference_global_types` after
global-type resolution and non-const widening. `CompiledProgram::compile_context`
is intentionally `#[serde(skip)]`, and
`restore_compile_context_from_program` currently substitutes an empty map.
Consequently a fresh program can infer a const-global reader as `Int64`, while
the same program loaded from `.sjvmbc` reports `Any` through
`Base.infer_return_type` and `Base.return_types`.

The existing #10265 parity assertion and #10462 restore scoreboard document this
as an Issue-tracked exception. They must become equality gates when this design
lands.

## Constraints

- Preserve the decision in `docs/vm/COMPILE_CONTEXT_REHYDRATION.md`:
  inference globals are a persisted snapshot, not a structural projection.
- Do not serialize the complete `RuntimeCompileContext`; its large structural
  state remains reconstructed from the round-tripped `Program`.
- Serialized bytes must be deterministic across process-local `HashMap` seeds.
- Cover manual serde restore, the custom Base-cache section format, and
  `.sjvmbc` load.
- Leave seeded-cache context activation to #10335; this change persists the
  data but does not invent a second restore entry point.

## Chosen representation

Add this field to `CompiledProgram`:

```rust
#[serde(default)]
pub inference_global_types_snapshot: Vec<(String, ValueType)>,
```

At finalization, copy the map from the already-built `compile_context`, sort by
binding name, and store the vector. A sorted vector is deterministic and avoids
teaching the lower `subset_julia_vm_bytecode` crate about the compiler crate's
sorted-`HashMap` serializer.

On restore, collect that vector back into
`RuntimeCompileContext::inference_global_types`. The snapshot comes from the
same fresh context that runtime reflection consumed before serialization, so no
optimizer state is reconstructed or approximated.

The custom Base-cache serializer must add an explicit
`compiled.inference_global_types_snapshot` section; whole-struct serde already
carries the field for `.sjvmbc` and the manual parity lane.

## Rejected alternatives

### Re-run global inference from `Program`

Rejected because the fresh value depends on optimized main/module statements
and non-const widening. The serialized `Program` does not contain that exact
state, so re-running a similar scan would create a second semantic algorithm.

### Serialize `RuntimeCompileContext` wholesale

Rejected because #3973 deliberately excludes the full prelude context from the
wire for startup cost and owner-ID reconstruction. #10333 needs one derived map,
not a reversal of that boundary.

### Persist a `HashMap` directly

Rejected because ordinary `HashMap` serde order depends on the per-process hash
seed. The cache format requires deterministic bytes.

## Tests

1. Replace the #10333 exemption in
   `restored_compile_context_matches_fresh_compile_10265` with exact map
   equality.
2. Remove `InferenceGlobalTypes` from the #10462 mismatch scoreboard and assert
   fresh/manual/`.sjvmbc` snapshots agree.
3. Add a `.sjvmbc` execution regression with one const and one mutable global:
   fresh and restored output must both be `Int64`, `Any`, `Any[Int64]`,
   `Any[Any]`.
4. Keep minimal/synthesized `CompiledProgram` constructors explicit with an
   empty snapshot.
5. Bump the Base-cache version and `.sjvmbc` version, refresh the schema
   fingerprint, and run cache determinism/round-trip tests.

## Documentation and compatibility

- `CACHE_VERSION` becomes 162 because the custom compiled section gains a
  positional payload.
- `.sjvmbc` `VERSION` becomes 5 because `SerializedVmBytecode` carries the new
  `CompiledProgram` field.
- `CACHE_ARCHITECTURE.md` and `COMPILE_CONTEXT_REHYDRATION.md` move #10333 from
  tracked gap to persisted-snapshot implementation.
- `STATUS.md` and `DONE.md` record the fresh/restore reflection parity result.
