# Where-binder environment and scoped type resolution

Last updated: 2026-07-14

Tracks: Issue #10436. Related: #10459, #10460, #10279, #10261, #10049.

## Problem

Milestone 76 cleared the concrete symptom backlog with local fixes and
regression fixtures, but several paths still answer the same question in
different ways:

> In this type context, does the spelling `T` mean a newly introduced
> `where` binder, an outer binder, a global type, a qualified global, or an
> undefined name?

Today that resolution is split across:

- function-signature lowering (`lowering/function/where_clause.rs` and
  `lowering/function/signature.rs`);
- value-position `where` and type-alias lowering (`lowering/stmt/assignment.rs`,
  `lowering/type_alias.rs`, and expression lowering helpers);
- `TypeExpr` / `JuliaType` / `CoreType` conversion paths
  (`TypeExpr::from_name`, `JuliaType::from_name_or_struct`,
  `CoreType::from_julia_name`);
- post-hoc tree rewrites such as `CoreType::rebind_where_binders`;
- subtype / dispatch binding scopes (`type_core/match.rs`,
  `dispatch_resolver/core_match.rs`);
- runtime reflection and `UnionAll` projection caches.

The symptom fixes made the reported cases unreachable, but the architecture is
still fragile: a new syntactic surface can bypass one of the repairs, or a
display string can erase the information needed to distinguish bare vs
qualified names and inner vs outer same-name binders.

## Target invariant

All type-bearing lowering paths should share one lexical binder environment.
After lowering, semantic phases must receive structured references, not raw
names that need to be reinterpreted.

The invariant:

1. A binder's bound is resolved in the enclosing environment, before the binder
   shadows its own spelling.
2. The binder is visible in the body/signature positions it scopes over, not in
   unrelated sibling positions.
3. Same-spelling binders from different lexical owners are distinct by identity.
4. A qualified global such as `Core.Builtin` is not captured by a bare
   `where Builtin` binder.
5. Undefined bound names are rejected at the lowering/resolution boundary with a
   precise span; they are not accepted as opaque `Struct(String)` placeholders.
6. Display strings are diagnostics only. They may be produced from a resolved
   node, but downstream equality, subtype, dispatch, reflection, and cache
   serialization must not depend on reparsing that display string.

## Proposed environment model

The environment should live in the lowest shared layer that can be used by
lowering and type construction without depending on VM runtime state. Exact Rust
names are intentionally provisional; the important part is ownership and data
flow.

```rust
struct TypeBinderEnv {
    frames: Vec<TypeBinderFrame>,
}

struct TypeBinderFrame {
    owner: TypeOwnerId,
    binders: Vec<TypeBinder>,
}

struct TypeBinder {
    id: TypeVarId,
    name: InternedStr,
    lower: Option<CoreTypeRef>,
    upper: Option<CoreTypeRef>,
    span: Span,
}

enum ResolvedTypeRef {
    Binder(TypeVarId),
    GlobalType(TypeIdentity),
    QualifiedGlobal(ModulePath, TypeIdentity),
    RuntimeValue(TypeExpr),
}
```

`TypeVarId` is the owner-scoped identity from #10459. `CoreTypeRef` is the
structured target from #10460. The environment may keep a by-name lookup stack
for lexical resolution, but that stack is not the semantic identity.

## Implemented routing

The lowering-side routing is now centralized in
`lowering/type_binder_env.rs`. One RAII frame stack is shared by:

- signature parsing and type-alias shadow exclusion;
- full- and short-form function-body lowering;
- nested closures and macro-return expression lowering that query the live
  `LambdaContext`.

The former `type_alias::EXCLUDED_PARAMS` and
`LambdaContext::active_type_params` stacks were retired. The existing
`check_name_based_lookup.sh` audit keeps both legacy field names at zero and its
negative self-test proves a parallel stack is rejected. Frames retain complete
`TypeParam` declarations and nearest-frame lookup, so nested same-name binders
preserve lexical shadowing and their declared bounds at the lowering boundary.

Semantic identity is not a name lookup: `CoreTypeVarId` and identity-keyed
`TypeVarBindingState` provide that layer, while the remaining display-string to
structured-type adapters are explicitly tracked by #10460.

## Lowering algorithm

For a `where` form:

1. Parse binder declarations into unresolved `(name, lower_expr, upper_expr,
   span)` records without mutating the active environment.
2. Resolve each bound expression against the enclosing environment.
   - This is what prevents `T<:T` from resolving the right-hand `T` to the
     binder being introduced.
   - Undefined names fail here unless the grammar position explicitly permits a
     runtime value expression.
3. Allocate `TypeVarId`s in source order under the current owner.
4. Push a new binder frame containing those IDs and resolved bounds.
5. Resolve the scoped body/signature under the extended environment.
6. Emit a structured `UnionAll` / signature node whose binder references are IDs
   and whose bounds are structured type refs.

For multi-binder syntax, preserve upstream's nesting order: the leftmost binder
is the outermost wrapper. Bound expressions still resolve before the binder being
declared becomes visible to itself.

## Refactor plan

| Area | Current shape | Target |
|---|---|---|
| `lowering/function/where_clause.rs` | **Routed:** pushes complete `TypeParam` frames into the shared environment before parsing the scoped signature. | The remaining `JuliaType` display projection is retired under #10460. |
| `lowering/function/signature.rs` | Expands annotations through `JuliaType::from_name_or_struct` and post-pass conversion. | Resolve annotations directly to `ResolvedTypeRef` / `CoreType`; reject undefined bound names before method-table insertion. |
| `lowering/stmt/assignment.rs` and `lowering/type_alias.rs` | Model type aliases and value-position `where` as runtime `UnionAll` construction plus string-keyed lookup metadata. | Reuse the same environment to construct the runtime `UnionAll` value and its compile-time structured projection. |
| `TypeExpr::from_name` | Treats names as either type parameters or nominal strings. | Accept an explicit resolver/environment; bare strings are lexical lookup inputs only. |
| `JuliaType::from_name_or_struct` / `CoreType::from_julia_name` | Parse display strings back into semantic structure. | Remain as source/diagnostic compatibility boundaries, not semantic paths in lowering, dispatch, or reflection. |
| `CoreType::rebind_where_binders` | Rewrites a built tree by matching binder names after parsing. | Retired once lowering produces `CoreType::TypeVar(TypeVarId)` references directly. |
| `type_core/match.rs` / `dispatch_resolver/core_match.rs` | **Identity migrated:** semantic binding state is keyed by `CoreTypeVarId`; names remain only for lexical lookup/display. | Preserve the identity-keyed invariant while #10460 retires string representation bridges. |
| Runtime reflection | Projection caches can encode owners as rendered body strings. | Runtime type objects reference the same owner-scoped binder IDs and structured graph used by dispatch. |

## Regression matrix

The migration is not complete until the same matrix is covered through function
signatures, value-position `where`, runtime reflection, and cache restore.

| Class | Example shape | Existing symptom anchors |
|---|---|---|
| Nested same-name binders | `Tuple{T, Vector{T} where T} where T` | #10302, #10279 |
| Builtin-name shadowing | `Vector{Float64} where Float64<:Real` | #10407, #10231 |
| Qualified global under bare binder | `Vector{Core.Builtin} where Builtin<:Function` | #10280 |
| Dependent chained bounds | `(Vector{S} where S<:U) where U<:Real` | #10410 |
| Undefined declaration-position bound | `f(x::T) where T<:MissingName` | #10396 |
| Anonymous covariant shorthand with undefined name | `Vector{<:MissingName}` | #10373 |
| Value-position nested where | `(Tuple{T} where T) where T` | #10303 |
| Type-object reflection identity | `.var`, `.body.parameters`, `===` for projected TypeVars | #10420, #10603 |
| Apply-type / partial application | `Core.apply_type(Vector, T)` under scoped binders | #10422, #10623, #10635 |
| Fresh vs cache restore | Same lowered method/type alias after cache serialization | #10459 / #10460 |

Each row should assert semantic behavior separately from printed display:
`==`/`===`/`isa`/`<:`/`supertype`/`typeof` where applicable, plus upstream
Julia parity for the surface syntax.

The current executable matrix is distributed across the focused fixtures that
originally exposed each surface: `scoped_typevar_identity_10279.jl`,
`where_binder_shadow_scope_10100.jl`,
`where_binder_signature_builtin_shadow_10942.jl`,
`where_binder_body_builtin_shadow_10934.jl`,
`where_binder_closure_body_11031.jl`,
`where_chained_bound_isa_10410.jl`,
`where_bound_undefvarerror_10226.jl`,
`where_decl_bound_undefvarerror_10396.jl`, and
`typevar_projection_structural_key_10987.jl`. The lowering environment has unit
coverage for nearest-frame same-name resolution and scope restoration; the
structured substitution path separately proves that two `CoreTypeVar`s with
the same display name but different scoped IDs cannot capture each other.

## Interaction with sibling epics

- #10459 provides identity: `TypeVarId`, `StructId`, and related owner-scoped
  IDs. This document assumes those IDs exist; it defines where they are allocated
  and how lexical resolution feeds them.
- #10460 provides representation preservation: once a name is resolved to an ID
  and structured type graph, later phases must not collapse it to
  `JuliaType::name()` and reparse it.
- #10436 owns the routing: all syntax that introduces or consumes `where`
  binders should use one environment and one set of scoping rules.

## Non-goals

- Do not implement all of Julia's subtype algorithm in one PR.
- Do not introduce package-name or module-name special cases.
- Do not make display strings globally stable IDs.
- Do not remove compatibility projections before their callers have structured
  replacements and cache migrations.
