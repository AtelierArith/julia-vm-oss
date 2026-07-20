# Package-Cache Nominal Registration Replay Design

**Issue:** #11280
**Date:** 2026-07-16

## Context

The package loader persists each lowered package as a serialized Core-IR
`Module` in a `.ji.json` file. A fresh load runs parser and lowering work before
the resulting `Module` enters the user `Program`; a cache hit deserializes the
same `Module` and skips those front-end passes. The serialized IR preserves
declarations, but thread-local nominal registration is not part of the payload.

`subset_julia_vm_types::types::register_type_name` records both a declaration's
bare family and its qualified owner. Dispatch, inference, reflection, and type
display consult that registry. When two modules declare the same family tail,
the registry activates owner-sensitive comparison. Skipping package-owner
registration on a cache hit can therefore make identical Core IR behave
differently from a fresh load.

PR #11261 contained the observed `AbstractAlgebra.Integers` failure by
recovering the unique qualified owner for a structurally matching parametric
definition. Issue #11280 closes the more general cache-boundary hole so future
registry consumers do not need equivalent recovery logic.

## Selected Approach

Add one loader-local, idempotent post-load pass:

```text
validated package source/cache entry
            |
            v
        lowered Module
            |
            v
load and validate dependencies
            |
            v
register_module_nominal_types(Module)
            |
            v
commit Module to PackageLoader.loaded
```

The pass walks the root module and nested submodules. At each level it builds
the same owner path as the compiler's existing `collect_module_structs` helper
and registers these declarations:

- concrete and parametric `StructDef` families;
- `AbstractTypeDef` families;
- `PrimitiveTypeDef` families.

Type aliases are deliberately excluded. `register_type_name` represents
nominal declarations, while aliases already live in `Module.type_aliases` and
are reconstructed by the compile-context alias path.

The pass runs for both fresh and cache-restored modules. This gives both lanes
one post-load authority instead of depending on whether a front-end side effect
happened during parsing. It runs only after dependencies load successfully and
immediately before the module is committed to `PackageLoader.loaded`, so a
failed load does not leave new nominal registrations behind. Set insertion
makes repeated registration harmless.

## Alternatives Rejected

### Generic cache-replay hook or trait

A framework would need ordering, failure, and rollback contracts before there
is a second replayable side effect. That abstraction would increase the loader
surface without strengthening #11280. A focused function can be promoted later
if a second durable post-load reconstruction appears.

### Serialize the thread-local registry into `.ji.json`

The registry is cumulative across Base, user code, and multiple packages. A
snapshot would capture unrelated load history and make the payload depend on
package order. The declarations already present in `Module` are the canonical,
order-independent source for reconstruction.

### Keep the #11261 recovery as the only guard

That recovery protects one parametric-instantiation consumer. Other consumers
of `REGISTERED_TYPE_NAMES` and `REGISTERED_QUALIFIED_TYPE_FAMILIES` would still
observe different state. Replay at the cache boundary removes the split once.

## Front-End Side-Effect Inventory

The production parser/lowering scan found four thread-local state owners:

| State | Lifetime | Post-lowering action |
|---|---|---|
| generated-unquote mode | scoped guard while lowering generated bodies | none; not semantic state after lowering |
| type-alias / declared-type / runtime-type-binding tables | one source-lowering scope; materialized into Core IR bindings and `Module.type_aliases` | compile paths reconstruct from IR; do not replay the transient tables |
| type-binder environment frames | lexical signature/type-expression scope | none; binders are represented in lowered `TypeExpr` / function metadata |
| quote dollar-preservation mode | scoped CST-to-constructor conversion | none; result is already represented in the lowered expression |

The nominal type registry is different: it is intentionally read after
lowering by dispatch/type consumers and its inputs are durable declarations in
the restored `Module`. It therefore receives the explicit post-load replay.

## Testing

TDD starts with a loader cache-hit regression using a family namespace unique
to Issue #11280:

1. register `Base.<family>` owners for one concrete struct, one parametric
   struct, one abstract type, one primitive type, and one nested-module struct;
2. create a valid `.ji.json` whose serialized `Module` contains the matching
   package-owned declarations, while the package source itself contains none;
3. prove every family is non-colliding before loading;
4. load through `PackageLoader` and prove every family becomes a qualified
   owner collision.

Because the declarations exist only in the cache payload, the test also proves
that the cache-hit branch—not source parsing—performed the replay. Before the
implementation it fails on the post-load collision assertions. Repeated
registration is harmless by construction because the registry stores both
families and qualified owners in `HashSet`s.

Verification then covers:

- the focused loader unit test under `release-fast`;
- the existing AbstractAlgebra fixture with an empty cache and a restored
  `.ji.json` cache;
- formatting, source-only audits, and the default Clippy lane;
- the full release nextest suite required for loader/compiler changes;
- the guarded PR gate on the exact current `main` and PR head.

## Documentation

`docs/vm/CACHE_ARCHITECTURE.md` will record nominal declarations as a
reconstructed cache-boundary side effect. `docs/vm/CHECKLISTS.md` will require
future serialized Core-IR loaders to inventory ambient front-end state
and reconstruct every post-lowering consumer from validated IR before commit.
`STATUS.md` and `DONE.md` will record the completed #11280 slice.
