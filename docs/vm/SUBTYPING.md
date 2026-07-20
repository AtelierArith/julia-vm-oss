# Subtyping Algorithm: Upstream Shape vs. sjulia (Issue #10049)

*Written 2026-07-10 as slice A of tech-debt epic #10049.*

This document does two things:

1. Describes the **algorithm shape** upstream Julia's `<:` uses
   (`julia/src/subtype.c`), and
2. Maps **where sjulia's `<:` implementation structurally diverges** from
   that shape, anchoring each divergence to the concrete bug(s) it caused.

Every claim about sjulia behavior below is either a direct citation of
`path:line` in this repository, or a command actually run against the
current `HEAD` (commit `c3ba98f29` at the time of writing) and upstream
`julia` 1.12 side by side. Where a cited bug (#9841, #9839, #9468, #9746,
#8806, #8804) is now CLOSED, its original MWE was **re-run against current
sjulia and confirmed to still agree with upstream** — see
["Verification: closed bugs, re-run"](#verification-closed-bugs-re-run)
below. The point of this document is not "sjulia currently gives wrong
answers on these five inputs" (it doesn't); it is "the architectural gap
that produced all five bugs is still there, and will produce the next one."
That is also the thesis of #10049 itself ("Why ad-hoc fixes keep recurring").

For the companion differential regression instrument (slice E), see
["The differential subtype matrix (slice E)"](#the-differential-subtype-matrix-slice-e)
below.

This document is scoped to the **subtype judgment** (`A <: B`) and, briefly,
`typejoin`. It does not restate `docs/vm/TYPE_SYSTEM.md` (the four type
representations), `docs/vm/PROMOTION.md` (numeric promotion), or
`docs/vm/LATTICE_TYPE.md` (the compiler's inference lattice) — see those for
the surrounding architecture.

## Upstream's algorithm shape

Read: `julia/src/subtype.c:1-21` (file header), `:43-119` (state structs),
`:757-851` (`var_lt`/`var_gt`/`subtype_var`), `:955-1048`
(`subtype_unionall`), `:1385-1566` (`subtype`, the main dispatch).

The file header states the algorithm directly:

> Uses the algorithm described in section 4.2.2 of
> https://github.com/JeffBezanson/phdthesis/
> This code adds the following features to the core algorithm:
> - Type variables can be restricted to range over only concrete types.
> - Diagonal rule: a type variable is concrete if it occurs more than once
>   in covariant position, and never in invariant position. [...]
> - Type{T}<:S if isa(T,S). [...]

Structurally, upstream's `<:` is **one recursive function, `subtype`**
(`subtype.c:1385`), driven by a **mutable environment** (`jl_stenv_t`,
`subtype.c:102-119`) that threads a **linked list of variable bindings**
(`jl_varbinding_t`, `subtype.c:67-91`) through the whole comparison:

```c
typedef struct jl_varbinding_t {
    jl_tvar_t *var;
    jl_value_t *lb;             // current lower bound (narrows as we recurse)
    jl_value_t *ub;              // current upper bound (narrows as we recurse)
    int8_t right;                // came from the right side of A <: B ("exists")
    int8_t occurs_inv;            // occurs in invariant position
    int8_t occurs_cov;            // # of occurrences in covariant position
    int8_t concrete;              // forced concrete by the diagonal rule
    ...
    struct jl_varbinding_t *prev;
} jl_varbinding_t;
```

The shape that matters for this document:

- **One environment, pushed per `UnionAll`.** `subtype_unionall`
  (`subtype.c:955`) is called for *either* side of `<:` (the `R` flag says
  which) and pushes exactly one `jl_varbinding_t` onto `e->vars`
  (`subtype.c:958-961`), regardless of whether the `UnionAll` came from the
  left or the right of `<:`. There is one code path for both directions, not
  two.
- **Bounds narrow incrementally as the comparison recurses.** `var_lt`
  (`subtype.c:757`) and `var_gt` (`subtype.c:798`) *mutate* `bb->ub` /
  `bb->lb` in place (`subtype.c:783`, `:814`) every time the same variable is
  encountered again deeper in the type. A variable's final bound is a
  fixpoint of every occurrence, not a static syntactic bound read once.
- **Forall vs. exists is a runtime flag on the binding (`right`), not a
  positional distinction.** A variable bound by a `UnionAll` on the left of
  `<:` is `∀` (upstream comment: "the bounds of left-side variables never
  change" — `subtype.c:1464-1467`); one from the right is `∃`, and its bounds
  are exactly the ones `var_lt`/`var_gt` narrow. Both flow through the same
  `subtype_var` (`subtype.c:829`) dispatcher.
- **The diagonal rule is computed from the accumulated environment**
  (`vb.occurs_cov > 1 && !var_occurs_invariant(...)`, `subtype.c:979`) *after*
  the recursive comparison returns, then it double-checks concreteness of the
  narrowed lower bound (`subtype.c:980-994`).

None of this is expressible as a single non-mutating pattern match over two
static ASTs; it requires an environment that both sides of the comparison
read and write as they recurse.

## sjulia's current implementation

### Where it lives

The engine sjulia's `<:` operator and method dispatch actually go through is
`CoreType::is_subtype_of` in `subset_julia_vm_types/src/inference_core/`:

- `CoreType` (the structured type shape): `type_core/repr.rs:211-243`.
- `CoreTypeVar` (the TypeVar representation): `type_core/repr.rs:171-177`.
- The subtype judgment itself: `type_core/subtype.rs:21-270`
  (`is_subtype_of_with_lookup`).
- The UnionAll *pattern-matching* half (used only when the UnionAll is on
  the right of `<:`): `type_core/match.rs:23-38` and `:461-746`
  (`core_type_matches_pattern_with_lookup`).
- `JuliaType::is_subtype_of` (`types/julia_type/comparison.rs:26-28`) is a
  thin facade over the same engine; `CoreSubtypeEngine`
  (`inference_core/subtype.rs:14-48`) is a second, even thinner facade used
  by struct-hierarchy-aware call sites.

**A third, unrelated "is_subtype" exists and must not be confused with the
above**: `LatticeType::is_subtype` / `lattice_is_subtype`
(`subset_julia_vm_types/src/runtime_types/lattice.rs:551-553`, `:866-868`,
`:1751-1826`) is the compiler's *inference* lattice (`Top`/`Bottom`/`Const`/
`PartialStruct`/`Conditional`), used for flow-sensitive type narrowing during
compilation — not what a user's `A <: B` or method dispatch evaluates. It has
its own, structurally different, `⊑` relation and is out of scope here (see
`docs/vm/LATTICE_TYPE.md`).

### Structural walkthrough

`is_subtype_of_with_lookup` (`type_core/subtype.rs:29-270`) is a single
`match (self, other) { ... }` over `CoreType` pairs — a syntax-directed
translation with **no environment parameter**. The two `TypeVar`-related
arms:

```rust
// type_core/subtype.rs:159-167
(Self::TypeVar(var), _) => var
    .upper_bound
    .as_deref()
    .is_some_and(|ub| ub.is_subtype_of_with_lookup(other, hierarchy)),
(Self::Value(_), _) => false,
(_, Self::TypeVar(var)) => var
    .upper_bound
    .as_deref()
    .is_none_or(|ub| self.is_subtype_of_with_lookup(ub, hierarchy)),
```

and the UnionAll arms:

```rust
// type_core/subtype.rs:168-177
(Self::UnionAll { var, body }, _) => {
    substitute_typevar_bound(body, var).is_subtype_of_with_lookup(other, hierarchy)
}
(_, Self::UnionAll { .. }) => match hierarchy {
    Some(hierarchy) => self.matches_unionall_pattern_with_hierarchy(other, hierarchy),
    None => self.matches_unionall_pattern(other),
},
```

## Divergences from upstream, anchored to bugs

### 1. Two unrelated implementations for "UnionAll on the left" vs. "UnionAll on the right"

Upstream's `subtype_unionall` (`julia/src/subtype.c:955`) is **one function**
called with an `R` flag for either side. sjulia instead **forks into two
independently-written code paths** depending purely on which side of `<:`
the `UnionAll` sits:

- **Left** (`(B where V) <: C`): `substitute_typevar_bound`
  (`subset_julia_vm_types/src/inference_core/type_core.rs:1250-1310`) does a
  **syntactic substitution** — it walks `body` and rewrites every
  `Named(var.name)` / matching `TypeVar` node into `TypeVar(var)` verbatim
  (`type_core.rs:1252-1253`), then the caller falls through to the
  `TypeVar`-vs-`other` arm above, which reads **only `upper_bound`**. The
  variable's `lower_bound` is never consulted on this path, and there is no
  environment: nothing here narrows a bound across multiple occurrences of
  the same variable (the diagonal rule and multi-occurrence narrowing that
  `subtype_unionall`'s `jl_varbinding_t` performs, `subtype.c:958-995`, has
  no sjulia counterpart on this side).
- **Right** (`C <: (B where V)`): `matches_unionall_pattern`
  (`type_core/match.rs:11-38`) instead runs a **binding-collection pattern
  matcher**, `core_type_matches_pattern_with_lookup`
  (`type_core/match.rs:461-746`), through `TypeVarBindingState`
  (`match.rs:290-420`) which *does* check both `lower_bound` and
  `upper_bound` (`match.rs:316-327`) and *does* track covariant/invariant
  occurrence counts for a diagonal-rule check
  (`match.rs:341-347`, `satisfies_diagonal_rule`).

These two paths were written independently, check different bound fields,
and have no shared representation of "the current variable environment."
They happen to agree on the differential matrix in this PR (see below), but
every future construct that exercises one side and not the other is
validated by a different, hand-written implementation than upstream's single
algorithm — which is exactly the epic's root-cause framing ("individual
special-case fixes... not a single subtyping judgment").

### 2. TypeVar-bound checking is independently reimplemented at least four times

Beyond the two implementations above, the SAME logical operation — "does
this TypeVar's declared bound admit this candidate type?" — is implemented a
third and fourth time, with visibly different rules each time:

- `struct_param_matches_pattern_with_lookup`
  (`subset_julia_vm_types/src/inference_core/type_core.rs:2198-2220`), used
  only for invariant struct-parameter comparison
  (`struct_params_are_subtype_with_lookup`, `type_core.rs:2096-2196`), checks
  both bounds but does **no** occurrence tracking / diagonal rule at all —
  it's a fresh, narrower reimplementation of `bind_or_check`
  (divergence 2 above), not a call to it.
- `bind_or_check_runtime_type_var`
  (`subset_julia_vm_types/src/inference_core/dispatch_resolver.rs:3188-3213`)
  is the **method-dispatch-time** (not compile-time `<:`) bound check, over
  `JuliaType` rather than `CoreType`. It checks **only the upper bound**
  (`specificity::usable_upper_bound`, `dispatch_resolver.rs:3195-3200`) —
  this is the exact gate that under-enforced bounds for `Type{T} where
  {T<:Bound}` methods called with a parametric-struct `Type{...}` argument
  (Issue #9839: `fb(::Type{T}) where {T<:AbstractFloat}` matched
  `fb(Q{Int64})`, an argument no bound admits). #9839 is fixed for its
  specific MWE, but the gate that produced it — a fourth, upper-bound-only,
  `JuliaType`-level bound check distinct from the `CoreType`-level ones
  above — is unchanged, and a new construct that reaches it the same way
  will reproduce the same class of bug.

`CoreTypeVar` itself (`type_core/repr.rs:171-177`) has no independent
identity beyond its `name: String`:

```rust
// type_core/repr.rs:171-177
pub struct CoreTypeVar {
    pub name: String,
    pub lower_bound: Option<Box<CoreType>>,
    pub upper_bound: Option<Box<CoreType>>,
}
```

There is no scope id, no depth, nothing that would let two `CoreTypeVar`s
spelled `"T"` in unrelated `where` clauses be told apart structurally — every
comparison above matches variables **by name string**. This is the
structural cause behind Issue #9746 (scoped TypeVar name collisions):
distinct `T`s in distinct scopes were treated as the same variable wherever
a name-keyed `HashMap<String, CoreType>`/`HashMap<String, CoreTypeVar>` (e.g.
`match.rs:464`'s `scope: &mut HashMap<String, CoreTypeVar>`) was the source
of truth, rather than upstream's linked *stack* of bindings keyed by `jl_tvar_t*`
pointer identity (`subtype.c:67-91`, `lookup`, `subtype.c:124-136`, which
walks `prev` links comparing `b->var == v` by pointer, not name). Item B of
epic #10049 ("Replace name-based TypeVar identification with scoped unique
IDs") targets exactly this; it is out of scope for this document (slice A
only documents and maps the gap).

**Anchor found while building the slice-E matrix, fixed 2026-07-10 (Issue
#10100)**: name-only identity was not just a display/scoping nuisance — it
produced a crash when a `where` binder's name textually collided with an
existing type name:

```julia
x = Vector{Int64} where Int64<:Int64   # binder named "Int64", self-referential bound
```

Upstream evaluates this fine (`Vector{Int64} where Int64<:Int64`, a
degenerate but legal shadowed `UnionAll`). sjulia used to overflow the stack
and abort the process — even just *constructing* the type, no `<:`/`==`
needed. A non-self-referential collision (`Vector{Int64} where Int64<:Real`)
did not crash but silently dropped the `where` clause instead, returning
plain `Vector{Int64}`. Both were consequences of resolving the bound
expression through the same name-keyed lookup that the newly-introduced
binder itself occupied, instead of resolving it in the enclosing scope
before the binder shadows that name. Because the crashing case is an
**uncatchable** process abort, it cannot be represented as a `@test` row in
the slice-E fixture (a `try`/`catch` cannot recover from a Rust-level stack
overflow, and it would take down the whole nextest binary) — it is
regression-tested in a standalone fixture instead
(`types/where_binder_shadow_scope_10100.jl`), not the skiplist.

**Fix** (does not touch `CoreTypeVar`'s name-only identity — the full
scoped-ID refactor, epic #10049 item B, is still deferred): two independent,
additive changes, keeping with the "resolve the bound in the enclosing
scope" framing above rather than special-casing `Int64` or any other name:

- **Lowering** (`subset_julia_vm_lowering/src/lowering/expr/mod.rs`,
  `typevar_bound_value_expr`): a where-binder's bound expression is now
  resolved with that binder's own entry excluded from the visible scope, so
  a self-referential bound (`Int64<:Int64`, `T<:T`, any name) falls through
  to the ENCLOSING scope's name instead of finding — and recursing into —
  itself. This is what stopped the lowering-time (compile-time, before any
  VM execution) Rust-level infinite recursion.
- **Keep vs. drop** (`subset_julia_vm_vm/src/vm/builtins_types.rs`,
  `julia_type_references_typevar`): the "does the body reference the bound
  variable" check (upstream's `jl_type_unionall` no-op-if-unused rule) now
  also matches a concrete/abstract leaf (`Int64`, `Real`, ...) whose
  canonical name equals the binder, the same way it already matched a
  generic `Struct("T")`/`TypeVar("T")` node for a non-colliding binder. This
  is what stops the non-self-referential case from being silently dropped.
- **Structural correctness of the kept value** — the naive fix above still
  left the `UnionAll`'s body holding the RAW, unrebound concrete leaf (e.g.
  `VectorOf(Int64)`, not `VectorOf(Struct("Int64"))`), which made `==`/`isa`
  against the shadowed occurrence silently wrong (e.g. `(Vector{Int64} where
  Int64<:Real) == Vector{Int64}` incorrectly `true`; `Float64[1.0] isa
  (Vector{Int64} where Int64<:Real)` incorrectly `false`) — a NEW instance of
  this document's own divergence 2 theme, reachable only because this fix
  makes the "kept" `UnionAll` constructible at all. The actual rebind already
  existed for this purpose: `CoreType::rebind_where_binders`
  (`subset_julia_vm_types/src/inference_core/type_core.rs:727`, Issue #9464)
  already rewrites a `Named`/empty-`Struct`/`AbstractUser` leaf matching a
  `where` binder into a `CoreType::TypeVar` when a `JuliaType::UnionAll`
  converts to `CoreType` — it was just never extended to
  `CoreType::Primitive`/`CoreType::Abstract` leaves, so a binder shadowing a
  BUILTIN name (unlike a user struct or unresolved name) fell straight
  through untouched. Extending that one existing mechanism (rather than
  adding a second, JuliaType-level rebind) is what fixes `==`/`isa`/`<:` for
  the shadowed occurrence, matching the "single mechanism, not a fourth
  reimplementation" principle this document argues for.

**Follow-ups from #10231's codex review (2026-07-11)** — three narrower edge
cases were filed off #10231's review; one is fixed here, while broader
qualification-preservation work remains under #10459/#10460:

- **Qualified-vs-bare shadowing — FIXED (Issue #10280)**: upstream's lexical
  `where`-binder scoping shadows only the BARE (unqualified) spelling of the
  binder name; an explicitly module-qualified reference whose last component
  equals the binder (`Core.Builtin` under `where Builtin`, or a user/Base path
  like `Base.RefValue{Int64}`) is NOT shadowed. sjulia's keep-vs-drop check
  (`type_name_references_typevar`,
  `subset_julia_vm_vm/src/vm/builtins_types.rs`) tokenized `Core.Builtin` into
  `["Core", "Builtin"]` and matched the bare token `Builtin`, so it wrongly
  reported the body as referencing the binder and kept a spurious `UnionAll`
  (`typeof(Vector{Core.Builtin} where Builtin<:Function)` was `UnionAll`, not
  `DataType`). Fix: skip a token that is module-qualified (immediately
  preceded by `.`) so the binder is seen as unused and the `where` drops to
  the concrete `Vector{Core.Builtin}`. General over any qualified path, not a
  `Core.`-name special case. Regression:
  `types/where_binder_qualified_shadow_10280.jl`. **Residual tracked by
  #10459/#10460 — FIXED**: the source-body rebind walker now preserves a simple
  module-qualified nominal leaf at the structured `JuliaType` layer instead of
  first collapsing it with the bare spelling in `CoreType`. A mixed body such
  as `Tuple{Builtin, Core.Builtin} where Builtin<:Function` therefore captures
  only the bare occurrence. The fixture also pins the dependent nested-bound
  case where an inner TypeVar bound refers to the outer shadowing binder.

- **Nested same-name binder — scoped identity closure for #10279**: the old
  binding solver scope used a single
  `HashMap<String, CoreTypeVar>`, so an inner `where T` could overwrite an
  outer `where T` and the two binders shared one binding/diagonal counter.
  `CoreTypeVar` now carries a `scope_id`, and
  `inference_core/type_core/match.rs` pushes lexical `TypeVarScope` frames and
  keys `TypeVarBindingState` through `CoreTypeVarId`. The regression
  `Tuple{Int64, Vector{Float64}} <: (Tuple{T, Vector{T} where T} where T)` is
  pinned in `scoped_typevar_identity_10279.jl`. The distinct-name value-level
  `isa` regression #10410 and the runtime reflection/typejoin identity work
  #10261 are also closed. New name-only TypeVar scope maps are rejected by
  `check_name_based_lookup.sh` and its injected negative self-test. The broader
  module-owned struct/function/method ID migration remains #10459, rather than
  extending this TypeVar-focused bug-cluster umbrella indefinitely.

### 3. Variance is a fixed per-type lookup table, not a lattice-level tag

Upstream has no separate "variance" concept as data — covariance,
invariance, and the diagonal rule all fall out of the single subtype
algorithm's treatment of type parameters (`subtype.c:1549-1559`: struct
parameters are compared with `forall_exists_equal`, i.e. invariantly, except
`Tuple`, which gets its own `subtype_tuple` covariant path,
`subtype.c:1547-1548`).

sjulia instead has an explicit, **hardcoded, per-type** variance table,
`JuliaType::variance` (`subset_julia_vm_types/src/types/julia_type/mod.rs:467-490`):

```rust
// types/julia_type/mod.rs:475-490
pub fn variance(&self) -> Option<Variance> {
    match self {
        JuliaType::Tuple | JuliaType::TupleOf(_) => Some(Variance::Covariant),
        JuliaType::Array | JuliaType::VectorOf(_) | JuliaType::MatrixOf(_) => {
            Some(Variance::Invariant)
        }
        JuliaType::Dict | JuliaType::Set => Some(Variance::Invariant),
        JuliaType::Struct(_) => Some(Variance::Invariant),
        _ => None,
    }
}
```

and, separately, `TypeVarVariance` in the pattern matcher
(`type_core/match.rs:284-288`) has only **two** variants:

```rust
pub(super) enum TypeVarVariance {
    Covariant,
    Invariant,
}
```

There is no `Contravariant` variant. Contravariant syntax (`{>:T}`) is
instead handled as a spelling convention: `Vector{>:Int64}` parses to a bare
`Struct { name: "Vector", params: [TypeVar { lower_bound: Some(Int64),
upper_bound: None, .. }] }` (verified: `CoreType::from_julia_name` test at
`type_core.rs:3698-3730`, `contravariant_type_arg_parses_as_lower_bound_issue_9468`)
rather than as a genuine third variance value threaded through the lattice —
so contravariance only "works" in the specific code paths that happen to
read `lower_bound` (which, per divergence 2, is not all of them).

This hardcoded-table design is what made invariant-container bugs
case-by-case rather than structural:

- **#8806** (`Vector{Number}` dispatch accepting a `Vector{Int64}`
  argument): per the issue's own analysis, the `CoreType` **engine** answered
  the `<:` query correctly (`Tuple{Vector{Int64}} <: Tuple{Vector{Number}}`
  → `false`); the bug was in a *separate* dispatch-scoring matcher that
  deliberately loosened container-element matching ("abstract container
  coercion") independently of the engine's variance rules. This is a
  **dispatch-surface** bug, not a `<:`-engine bug — it lives outside the
  matrix this document's slice E adds (see below), and is a fifth place
  ("scoring path") that re-derives variance-like behavior ad hoc.
- **#8804** (`Tuple{Vector{Int64}} <: Tuple{Matrix}` was `true`, upstream
  `false`): the **bare** (non-tuple-wrapped) array-family arm correctly
  applies the invariant, dimension-aware comparison (`array_family_dim`,
  `type_core.rs:2114-2146`, fixed by #8560/PR #8584), but the **tuple-wrapped**
  path went through a *different* binding-aware matcher
  (`tuple_elements_match_with_bindings` → `array_family_pattern_params_match`,
  `match.rs:543-570`) whose bare-family acceptance arm erased dimensionality.
  Same variance rule, enforced in two structurally different places, one of
  which had a bug the other didn't.

### 4. `typejoin` is not built on the `<:` lattice at all

The `typejoin`/`promote_type` builtins users actually call are **pure
Julia**, `subset_julia_vm/src/julia/base/reflection.jl:443-550`, and are
structurally close to upstream's `julia/base/promotion.jl:24-141`: both walk
a supertype chain via `supertype()`/`a <: b` calls rather than touching any
Rust-level type representation directly. This part is *not* a divergence in
spirit — sjulia's `typejoin` short-circuits `a <: b`/`b <: a` exactly like
upstream (`reflection.jl:454-455`, fixed for Issue #9841's MWE — see
verification below) and walks the supertype chain the same way
(`reflection.jl:532-549` vs. `promotion.jl:100-140`).

**Update (2026-07-11, Issue #10091, PR #10225 — CLOSED/merged):** the
same-typename branch used to widen **all** parameters to the bare wrapper
the moment **any** parameter differed (`same = false` anywhere ⇒
`Core.apply_type(a)`), whereas upstream widens only the *differing*
parameter(s) to a fresh `where`-bound TypeVar and keeps every parameter that
agrees (`promotion.jl:100-140`, the `vars`/`UnionAll` accumulation). This is
now fixed: the branch walks the fully-generic wrapper's UnionAll chain,
substitutes the joined concrete value into agreeing positions and a fresh
`where`-bound TypeVar into differing ones, and rewraps — matching upstream
for both a differing-trailing-param and a differing-leading-param join.
Re-verified directly against upstream on commit `523611726` (current `main`):

```julia
struct PairT10049{A,B}; a::A; b::B; end
typejoin(PairT10049{Int64,Int64}, PairT10049{Int64,String})
# upstream: PairT10049{Int64}                (keeps the agreeing first param)
# sjulia:   PairT10049{Int64, B} where B     (same lattice element; sjulia's
#                                              `show` does not yet elide a
#                                              trailing free `where`, a
#                                              separate display-only gap,
#                                              Issue #10195)
typejoin(PairT10049{Int64,Float64}, PairT10049{String,Float64})
# upstream: PairT10049{A, Float64} where A
# sjulia:   PairT10049{A, Float64} where A   (exact match)
```

Regression coverage:
`subset_julia_vm/tests/fixtures/types/typejoin_partial_param_widen_10091.jl`.

### 4a. Dependent bound (`B<:A`) identity closure (Issues #10252 / #10261)

Upstream represents a dependent bound as a reference to the same `TypeVar`
object bound by the earlier `UnionAll`. sjulia now preserves that invariant
through owner-scoped runtime IDs:

```julia
struct Dep3{A, B<:A, C<:B} end
w = Dep3
a = w.var
b = w.body.var
c = w.body.body.var
b.ub === a  # true
c.ub === b  # true
```

The projection domain is keyed by a structural owner schema (`CoreType`), binder
depth from the final body, and binder metadata. Only IDs allocated by this
projection cache are normalized in the owner key; external free-TypeVar IDs and
Array value parameters such as rank remain semantic input. Depth is stable when
an inner UnionAll suffix is observed first, and a name-only dependent bound
resolves to the lexically nearest outer depth, so nested same-name binders remain
distinct without HashMap iteration-order dependence.

Before `Core.apply_type` validates a wrapper carrying projected binders, its
legacy `UnionAll` chain is promoted to `RuntimeUnionAll`: binder, bound, and body
references therefore share the same IDs. Applied concrete arguments are
substituted into later bounds by ID. Matching upstream `within_typevar`, bound
validation is deferred while the argument as supplied or either bound endpoint
still contains a free TypeVar; one free endpoint defers the complete interval
check, and the argument check happens before earlier-binder substitution,
matching upstream `within_typevar`. This avoids incorrectly
collapsing invariant composite bounds to an endpoint such as `Vector{Any}`.
Structured runtime parameters also preserve TypeVar identity for arbitrary
Array ranks, not only Vector/Matrix. The Pure Julia `typejoin` helper
consequently matches dependent bounds by `===`, not by `typename` or binder
spelling.

The former cross-struct collision now retains upstream precision:

```julia
struct Dep2{A, B<:A} end
struct Dep3{A, B<:A, C<:B} end
typejoin(Dep2{Int64,Int64}, Dep2{Float64,Float64})              # Dep2 (correct both interpreters)
typejoin(Dep3{Number,Int64,Int64}, Dep3{Number,Float64,Float64}) # run AFTER the Dep2 call above
# upstream: Dep3{Number, B, C} where {B<:Number, C<:B}
# sjulia:   Dep3{Number, B, C} where {B<:Number, C<:B}
```

The final `a <: result && b <: result` check remains as a general fail-closed
guard, not as a name-based workaround. W-65 is retired. The broader migrations
of remaining string-keyed type maps and display-name lookups continue under
#10459/#10460; they are not blockers for this completed TypeVar vertical slice.

There is also a Rust-side `CoreType::typejoin`
(`subset_julia_vm_types/src/inference_core/type_core.rs:401-428`), but it is
**not** the function backing the user-facing builtin — it is used only
internally (`Conditional` lattice widening in `runtime_types/lattice.rs:2203`,
AoT type widening in `subset_julia_vm/src/aot/types.rs:367`). It is even more
conservative than the Julia-level one: any non-subtype pair that isn't a
same-arity `Tuple` collapses straight to `Union{self, other}`
(`type_core.rs:427`, `normalize_union`) with no supertype-chain walk at all.
Documenting this split matters because a future contributor grepping
`typejoin` in Rust will find the *wrong* implementation for "what does the
`typejoin` builtin actually do" questions.

## Divergence-to-bug map

| Divergence | Anchor bug(s) | Current status |
|---|---|---|
| Two unrelated code paths for left- vs. right-side `UnionAll` | (structural; no single bug, but shape of #9839/#9468 fixes) | Live — both paths still separately maintained |
| TypeVar bound checking reimplemented ≥4 times, inconsistent bound coverage | #9839 (dispatch-time upper-bound-only gate let a `Type{T} where {T<:Bound}` method match an excluded `Type{ParametricStruct}` arg) | #9839 MWE fixed; underlying 4-way duplication unchanged |
| `CoreTypeVar` has no scope identity, only a name string | #9746 (same-name TypeVar in different scopes collided), #10279 (same-name nested `where` binders shared matcher bindings) | **Closed** — `CoreTypeVarId` projects scoped and rigid identities; `TypeVarBindingState` keys bindings/diagonal occurrences by that identity and uses a lexical scope stack. Runtime reflection/typejoin identity is closed under #10261; remaining cross-owner struct/function ID migration is #10459. |
| `CoreTypeVar` name string can collide with an existing type name, corrupting bound resolution | **#10100 (filed 2026-07-10)**: `Vector{Int64} where Int64<:Int64` stack-overflow crashes; `Vector{Int64} where Int64<:Real` silently drops the `where` | Fixed (#10231, 2026-07-10); matcher-side scoped identity now prevents same-name matcher bindings from sharing a slot, but parser/lowering name collision paths remain separate surfaces. |
| Keep-vs-drop / rebind cannot tell a module-qualified reference from a bare binder name | **#10280 (filed 2026-07-10)**: `Vector{Core.Builtin} where Builtin<:Function` wrongly kept a `UnionAll` (upstream: concrete `Vector{Core.Builtin}` DataType) | Keep-vs-drop fixed (2026-07-11) by skipping `.`-preceded tokens; mixed bare+qualified `rebind` residual belongs to #10459/#10460 |
| Name-keyed binder scope collapses nested same-name `where` binders | **#10274 (filed 2026-07-10)**: `(Vector{T} where T<:T) where T<:Real` — inner bound disconnected; `Float64[1.0] isa r` wrongly `false`; #10279 matcher regression `Tuple{T, Vector{T} where T} where T` | **Closed** — `match.rs` pushes lexical `TypeVarScope` frames and binds through `CoreTypeVarId`, so inner same-name binders do not reuse the outer binding. Distinct-name chained value-level `isa` #10410 is also fixed. |
| Variance is a hardcoded per-type table, no `Contravariant` tag | #9468 (`{>:T}` gave wrong answer) | #9468 MWE fixed via a bound-field convention, not a variance kind |
| Invariant container rule applied in the bare arm but not the tuple-wrapped arm | #8804 (`Tuple{Vector{Int64}} <: Tuple{Matrix}` was `true`) | Fixed; array-family-in-tuple has its own matcher, still separate from the bare arm |
| Invariant container rule bypassed by a separate dispatch-scoring matcher | #8806 (`f(x::Vector{Number})` accepted a `Vector{Int64}` argument) | Fixed; dispatch-scoring "abstract container coercion" is a still-separate surface from the `<:` engine |
| `typejoin`'s same-typename branch collapses ALL params instead of only the differing ones | #10091 (PR #10225) | **Closed/fixed** — per-parameter widening lands; see §4 above |
| Dependent `TypeVar` bounds lose binder identity across wrapper projection/application | #10252, umbrella **#10261** (related #10100, #10133, #10192) | **Closed/fixed** — owner-scoped runtime IDs preserve `B.ub === A` / `C.ub === B`; exact cross-struct `typejoin` precision restored; see §4a. Broader string-keyed map retirement remains #10459/#10460. |

## Verification: closed bugs, re-run

Each MWE below was re-run against `julia --startup-file=no` and
`target/dev-fast/sjulia` at commit `c3ba98f29` (2026-07-10). All six now
agree with upstream:

```
=== 9841 typejoin ===                    upstream   sjulia
typejoin(Complex{Float64}, Complex)      Complex    Complex
typejoin(Complex, Complex{Float64})      Complex    Complex
typejoin(Rational{Int64}, Rational)      Rational   Rational
promote_type(Complex{Float64}, Complex)  Complex    Complex

=== 9839 Type{T} where bound ===
fb(Q9839{Int64})   (T<:AbstractFloat / T<:Integer methods, neither admits Q9839{Int64})
  upstream: MethodError                  sjulia: MethodError
fb2(Rational{Int64})  (T<:AbstractFloat method; bound excludes Rational{Int64})
  upstream: MethodError                  sjulia: MethodError

=== 9468 contravariant ===
Circle9468{Real} <: Shape9468{>:Int64}   true       true

=== 8806 invariant Vector dispatch ===
f(x::Vector{Number})=...; f(x)=...; f([1,2])
                                          "any"      "any"

=== 8804 tuple-wrapped dimension ===
Tuple{Vector{Int64}} <: Tuple{Matrix}    false      false
Tuple{Matrix{Int64}} <: Tuple{Vector}    false      false
Vector{Int64} <: Matrix                  false      false
```

(`9746`'s original scoped-TypeVar collision MWE is exercised by the
already-registered fixture
`subset_julia_vm/tests/fixtures/types/typevar_name_collision_scope_9563.jl`,
not re-transcribed here.)

## The differential subtype matrix (slice E)

To keep future subtype fixes from regressing silently, and to give the
epic's "does the gap keep shrinking" question a concrete number, this PR
adds a differential property test mirroring the numeric-matrix precedent
(`scripts/gen_numeric_matrix_fixture.jl`, `docs/vm/CHECKLISTS.md` Issue
#8698):

- **Generator**: `scripts/gen_subtype_matrix_fixture.jl` (upstream-`julia`-only,
  deterministic — a fixed, hand-curated list of type-pair expressions, not
  randomized). Covers concrete numerics, abstract supertype hierarchy,
  invariant parametric-struct params, `Vector`/`Matrix`/`Tuple` combinations
  (including the #8804 shape), `Union` types, `UnionAll` with upper AND lower
  (`{>:T}`) bounds (including the #9468 shape), the diagonal rule
  (`Tuple{T,T} where T`), `UnionAll <: UnionAll` (both sides bind a
  TypeVar — the case divergence 1 above says is handled by two unrelated
  code paths), nested/chained `where` over a two-typevar struct, and
  `Type{T}`.
- **Oracle**: `subset_julia_vm/tests/fixtures/types/subtype_matrix_oracle_10049.tsv`
  — every candidate pair with upstream's verdict, for provenance/diffing.
- **Fixture**: `subset_julia_vm/tests/fixtures/types/subtype_matrix_oracle_10049.jl`
  — one `@test` per pair NOT in the skiplist, asserting `A <: B ==
  <upstream's verdict>`. Registered in `manifest.toml` as
  `types_subtype_matrix_oracle_10049`.
- **Skiplist**: `docs/vm/SUBTYPE_MATRIX_SKIPLIST.tsv` — pairs where sjulia
  disagrees with upstream, tracked as `(id, left_expr, right_expr,
  upstream_result, sjulia_result, issue, reason)`. **Currently empty** (see
  below).
- **Regeneration story**: when a subtype fix lands, re-run
  `julia --startup-file=no scripts/gen_subtype_matrix_fixture.jl` (after
  removing the newly-fixed pair's row from the skiplist) to have it
  re-appear as an asserted `@test`.

### Scope of the instrument

This matrix measures the **`<:` engine surface only**
(`CoreType::is_subtype_of` / `JuliaType::is_subtype_of`). Per divergence 3
above, `#8806`'s bug lived in the **dispatch-scoring** surface, whose `<:`
verdict already agreed with upstream — a pair like `Vector{Int64} <:
Vector{Number}` would not have caught that class of bug even before the fix,
because the engine was never wrong about it. The matrix is not a
substitute for dispatch-level differential testing; it is scoped to the
subtype judgment this document describes.

It is also scoped to inputs whose disagreement is **catchable**. Issue
#10100 (divergence 2, above) is a construction-time stack overflow — an
uncatchable process abort, not a `try`/`catch`-able `MethodError` or wrong
boolean — so a pair that triggers it can never appear as a `@test` row
without risking aborting the whole `fixture_tests` binary. It is tracked as
a standalone bug instead of a skiplist row.

### Current result: 82/82 pairs agree, 0 skiplisted

Running the generated fixture under both interpreters at commit `c3ba98f29`:

```
$ julia --startup-file=no subset_julia_vm/tests/fixtures/types/subtype_matrix_oracle_10049.jl
# 7 testsets, 82/82 passed

$ ./target/dev-fast/sjulia subset_julia_vm/tests/fixtures/types/subtype_matrix_oracle_10049.jl
# 7 testsets, 82/82 passed
```

Zero divergences is a legitimate result, not evidence the instrument lacks
teeth: it includes the exact shapes of #8804, #9468, the diagonal rule, and
`UnionAll <: UnionAll` on both sides — all of which now agree. It confirms
the epic's own framing: the **individual** subtype judgments upstream users
hit most often are now correct; what remains structurally wrong is the
**duplication** (divergences 1-3 above) that makes each fix local rather
than systemic, and the **adjacent** surfaces (dispatch scoring, `typejoin`)
that don't share the engine's correctness. As dispatch-surface and
`typejoin` differential coverage is added in later epic slices, expect this
matrix's skiplist to gain rows from those surfaces before it gains rows from
new `<:`-engine gaps.

## Epic completion update (2026-07-13)

The original slice left items B-D open. They are now covered by the shared
semantic core:

- **B, scoped TypeVar identity:** `CoreTypeVarId` and runtime
  `JuliaType::RuntimeTypeVar` carry identity through subtype, application,
  reflection, and serialization. The last VM-global `(name, upper-name)` cache
  has been removed; UnionAll projection identity is keyed by structural owner
  `CoreType`, so unrelated same-name binders cannot alias.
- **C, variance:** matcher positions explicitly use `TypeVarVariance`.
  Tuple positions are covariant; nominal struct/container parameters are
  invariant. Upstream `{>:T}` semantics are a TypeVar lower bound, not a
  contravariant user-type declaration, so no type-name-specific contravariant
  container table is introduced.
- **D, typejoin:** user-visible parametric joins retain agreeing parameters and
  widen only differing positions. The shared `CoreType::typejoin` now walks the
  same canonical builtin parent relation as subtype/reflection and performs the
  operation recursively for fixed Tuple elements.

Together with the upstream map (A), 82-case differential matrix (E), and the
listed regression fixtures, this completes Issue #10049. Broader migration of
module/struct/function identities remains separately scoped by #10459 and is
not a TypeVar identity fallback.
