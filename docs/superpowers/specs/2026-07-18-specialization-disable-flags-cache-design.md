# Specialization-Disable Flags Cache Design

## Problem

Issue #10334 identifies a semantic drift between fresh compilation and cache
restore. Fresh compilation derives three runtime-specialization safety flags
from resolved method tables:

- native `getindex` specialization must be disabled for user array overrides;
- native `setindex!` specialization must be disabled for user array overrides;
- direct field-access specialization must be disabled for user `getproperty`
  overrides.

`restore_compile_context_from_program` currently tries to rediscover those
facts from source IR. That mirror only walks top-level functions and recognizes
array receivers by spelling, so it misses module-owned methods and type aliases.
A restored program can therefore re-enable a fast path that bypasses user
dispatch.

## Decision

Persist the finalized fresh-compile decision instead of expanding the IR mirror.
Add a serializable, defaultable `SpecializationDisableFlags` value to
`CompiledProgram`. The value contains the three named booleans and is produced
once, immediately after the resolved method-table queries in `Pipeline::finalize`.
The transient `RuntimeCompileContext` receives the same value's fields.

Cache restore will copy the persisted value into the reconstructed
`RuntimeCompileContext`. It will not inspect function names, module trees, or
receiver annotations to recompute these flags. This makes the method table the
single semantic authority and removes an alias/module-sensitive duplicate
implementation.

The existing compile-context activation predicate remains unchanged. In
particular, a `getproperty` override alone does not activate specialization;
the persisted flag only matters when another existing condition creates a
runtime compile context.

## Serialization Boundaries

The snapshot must cross every `CompiledProgram` wire boundary:

1. whole-struct serde used by manual restore and `.sjvmbc`;
2. the custom Base-cache compiled payload;
3. test constructors and minimal compiled-program builders.

The custom Base-cache payload receives one named section for the flags.
`CACHE_VERSION` is bumped from 162 to 163, `.sjvmbc` `VERSION` from 5 to 6,
and the audited Base-cache schema fingerprint is regenerated. Old artifacts are
rejected by their existing version checks rather than interpreted under the new
layout.

## Testing

TDD starts by extending the #10265 parity corpus with module-local
`Base.getindex`, `Base.setindex!`, and `Base.getproperty` methods whose array
receiver is a local alias. Fresh compilation must set all three flags while the
current restore mirror produces false, giving a three-field RED failure.

After implementation:

- the #10265 fresh/restore parity assertion compares persisted values and its
  messages name Issue #10334 rather than claiming re-derivation;
- whole-struct serialization proves the flags survive with
  `compile_context == None` before restore;
- custom Base-cache serialization proves a non-default flag snapshot survives;
- an observable `.sjvmbc` regression is added if the specialization trigger can
  be expressed without relying on unrelated unsupported syntax;
- schema, source audits, clippy, focused tests, and the full release suite are
  required before the guarded merge.

## Alternatives Rejected

Expanding the restore walker to recurse through modules and resolve aliases
would retain two semantic authorities and drift again when method resolution
changes. Rebuilding complete method tables during restore would recover the
authority but adds avoidable load cost and complexity when the required output
is only three booleans. Persisting the finalized decision is both exact and
minimal.
