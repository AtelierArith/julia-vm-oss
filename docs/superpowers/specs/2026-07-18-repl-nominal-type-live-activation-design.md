# REPL Nominal Type Live Activation Design

**Issues:** #9784, #11635, #11651, #11652, #11654, #11655, #11656
**Date:** 2026-07-18
**Status:** Approved for implementation

## Objective

Remove the fresh full-recompile fallback for brand-new Main-owned
`abstract type`, `primitive type`, and `@enum` declarations. These nominal
types must be compiled against the retained REPL snapshot, reserved privately
on the held VM, published only when execution reaches their source position,
and committed to the persistent compiler/session state by the exact reached
prefix after a Julia-catchable error.

This is the next independently mergeable LV4b slice of #9784. It also fixes
#11635: nominal type values must not be visible before their declarations, and
the corresponding Main binding must be visible to `isdefined` after
publication. Parametric or inner-constructor structs, type redefinition,
module-owned declarations, aliases, macros, imports, and opaque runtime `eval`
remain separate #9784 slices.

## Upstream Shape

Upstream Julia constructs method/type metadata before publishing it, then
inserts the binding at top-level evaluation order. `jl_method_def` and
`jl_method_table_activate` use the same prepare-then-world-publication split for
methods. The `_abstracttype` / `_primitivetype` builtins call
`jl_new_abstracttype` / `jl_new_primitivetype` to create the datatype wrapper;
the lowered declaration then assigns its supertype and module binding at the
declaration's evaluation point. `@enum` expands to a primitive subtype plus
type/member bindings at the macro call's source position.

sjulia already follows this shape for concrete structs: compiled metadata is
reserved privately, `DefineEvalStruct` publishes it, and the reached activation
log drives error-prefix recovery. The new design generalizes that transaction
instead of introducing per-syntax fallback behavior.

## Scope and Eligibility

The live path admits an input only when every newly declared nominal type is:

- Main-owned and brand new in the persistent prefix;
- an abstract type, a non-parametric primitive type, or an `@enum`;
- structurally present as the exact new tail of the compiler's matching
  registry; and
- representable by a source-positioned activation marker in the appended main.

An input may interleave these declarations with already-supported functions,
concrete structs, expressions, and globals. Eligibility remains semantic;
compile-side extraction independently proves registry tails, marker order,
binding names, global slots, and activation metadata before the held VM is
taken or mutated.

The following remain fail-closed on the full path:

- same-name type redefinitions;
- parametric and inner-constructor structs;
- module-owned nominal types;
- type aliases and macro definitions;
- package/module/import state not yet admitted by LV5b; and
- opaque runtime `eval` mutations.

## Unified Activation Model

### Typed transaction log

Extend `ReplDefinitionActivation` with explicit abstract, primitive, and enum
variants. Extend `ReachedReplDefinitionPrefix` with per-family reached counts.
The activation sequence remains one interleaved typed log; separate totals are
not trusted to reconstruct chronology.

Add append-only bytecode markers for abstract and primitive declarations:

- `DefineEvalAbstractType(index)`
- `DefineEvalPrimitiveType(index)`

`RegisterEnum` already executes at the correct source position. Its execution
records the enum activation in the same typed log after registering the type
and before member stores run. No duplicate enum marker is added.

### Private reservations

The VM owns pending queues for abstract and primitive metadata, following the
existing concrete-struct reservation pattern. A declaration marker must match
the next expected index and registry length. Publication atomically updates:

- `Vm::abstract_types` plus `abstract_type_name_index` for abstract types, or
  `RuntimeCompileContext::primitive_types` for primitive types;
- `struct_hierarchy` parent edges;
- derived `type_ancestors` data;
- compile/runtime type-recognition surfaces;
- the Main type binding observed by `isdefined`; and
- dispatch/specialization caches affected by the new subtype relation.

Enum registration publishes its type binding and members at the existing
`RegisterEnum` instruction. A pre-run enum-registry snapshot is restored when
the live transaction is rejected or an internal/host error drops the VM, so
the thread-local formatting registry cannot outlive a rejected compiler state.

### Source visibility

Generalize the current pending concrete-type fence to every current-input
nominal type. `PushDataType`, module binding lookup, `isdefined`, constructors,
`isa`, reflection, and dispatch must treat a pending name as undefined. The
matching activation removes the pending state and publishes the Main binding.
This fixes both halves of #11635: no early type value and a real post-marker
binding.

The compiler may still use pre-collected metadata to type-check later source;
that compile-time knowledge never authorizes a runtime read before the marker.

## Compiler Snapshot and Recovery

`repl_relocatable_delta_compile` extracts exact new tails from
`CompiledProgram.abstract_types` and `CompiledProgram.primitive_types`, plus
the current-input enum definitions from source-ordered main statements. It
relocates the appended main and emits a typed activation sequence covering all
function and nominal-type markers.

On success, the persistent compile snapshot advances to the fully activated
registries. On a Julia-catchable error:

1. the VM validates the observed typed activation sequence against the planned
   prefix;
2. unreached pending metadata is discarded;
3. compiler registries, type-name surfaces, hierarchy projections, and source
   definitions are projected to the reached counts;
4. enum definitions and bindings are retained only when their `RegisterEnum`
   activation ran; and
5. `store_definitions` records exactly that same prefix.

Host cancellation, internal invariant failure, marker mismatch, or registry
misalignment drops the live VM, restores the enum-registry snapshot, and leaves
the prior persistent snapshot authoritative.

## Adversarial Review Closure

The completed implementation is also required to close four publication holes
found by adversarial review:

- every enum value read, including the statically lowered `instances(Enum)`
  path, must cross the pending enum-generation fence before `RegisterEnum`
  publishes that generation; a later same-named pending enum must not re-hide
  an already-active generation;
- the cache's newly introduced binding set must include abstract, primitive,
  enum-type, and enum-member names, so an older function containing an
  unresolved load or trap (`LoadAny`, `LoadGlobalAny`, or
  `ThrowUndefVarError`) forces the safe full-refresh path instead of remaining
  stale (#11651). Explicit global loads must resolve definition-owned nominal
  bindings after that refresh (#11655);
- a nominal declaration whose parent is still pending must raise
  `UndefVarError` before the child is published, leaving neither the child nor
  the later parent in the reached prefix; and
- enum type publication precedes enum-member constant validation, while a
  colliding member raises a Julia-catchable error without overwriting the
  existing global. Validation occurs at each member publication point, and
  recovery persists the complete enum metadata separately from the exact set
  of member constants whose stores completed, so a later full rebuild cannot
  revive the rejected or unreached stores (#11652).

The current transaction preserves the side effects produced by sjulia's
source-ordered enum-member stores. Matching Julia's `Dict`-driven macro
expansion order for a later member collision remains tracked by #11656.

Julia accepts nominal declarations inside `try`, but sjulia currently rejects
that syntax during lowering (#11654). The current activation transaction covers
uncaught declaration errors; caught continuation through a later declaration is
tracked by #11654 rather than being simulated by a runtime-only test that users
cannot yet express.

These are shared runtime/compiler authority rules, not syntax-specific
exceptions. They must cover all nominal families and all enum read/store paths.

## Verification

Tests use existing consolidated REPL/VM binaries. No new `tests/*.rs` binary is
created.

The red/green matrix includes:

1. declaration-before/after visibility for abstract, primitive, and enum types;
2. `isdefined(Main, name)` before and after each marker (#11635);
3. a struct subtype and method dispatch through a newly declared abstract type;
4. primitive construction, conversion, `sizeof`, and dispatch after activation;
5. enum type/member binding, display, construction, `instances`, and dispatch;
6. multiple interleaved function/concrete/abstract/primitive/enum definitions;
7. a catchable error after a reached prefix and before an unreached mixed suffix;
8. a non-catchable/rejected path proving enum registry restoration;
9. upstream Julia output parity for source-order behavior; and
10. `last_vm_build_nanos() == Some(0)` for every covered definition input and
    its subsequent reads, except the explicit #11651 safety fallback that must
    rebuild older unresolved callers;
11. `instances(PendingEnum)` remains undefined before `RegisterEnum`;
12. older unresolved callers refresh for every nominal and enum-member binding;
13. a pending parent rejects abstract, concrete, and primitive child
    publication; and
14. an enum-member collision preserves the old global while retaining the
    already-published enum type across both live reuse and a forced full rebuild;
15. a later same-named pending enum does not fence the already-published enum;
    and
16. separate cache cases cover every nominal binding family plus
    `LoadAny`, `LoadGlobalAny`, and `ThrowUndefVarError`.

Targeted release-fast tests run first. Final gates are formatting, instruction
wire/schema audits, source-only audits, default clippy, release sjulia, iOS
device/simulator builds, full release nextest, metamorphic equivalence, and the
guarded regular merge. Adding bytecode variants requires append-only wire IDs
and a Base-cache schema/version bump.

## Rejected Alternatives

### Separate activation protocols per declaration syntax

This duplicates marker ordering, rollback, binding publication, and hierarchy
maintenance. The runtime invariant is one nominal-type transaction, not three
unrelated syntax features.

### Publish all type registries before main and roll back afterward

Rollback cannot undo observations already made by earlier statements and
repeats the early-visibility bug in #11635. Publication must occur at the
source marker.

### Keep the fresh fallback and only compress its reconstruction mirrors

That does not retire the accumulated-source machinery required by #9784 and
leaves definition cost dependent on session length. The accepted outcome is a
real live transaction with zero VM rebuild time.

## Follow-on Order Within #9784

After this slice:

1. parametric and inner-constructor struct families;
2. type redefinition with identity-safe invalidation;
3. Base/preload-owned and module-owned method activation;
4. remaining package/import/macro/type-alias/baremodule state;
5. opaque runtime `eval` through the normal compile/VM transaction; and
6. deletion of reconstruction mirrors and accumulated-source fallback, then
   closure of #9784.
