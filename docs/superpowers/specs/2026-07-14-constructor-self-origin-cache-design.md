# Constructor Self-Origin Cache Design

Date: 2026-07-14
Issues: #10974, #10969
Related: #10959, #10967, #10968, #10971, #10976

> **Completion amendment (2026-07-14, PR #11012):** the sibling-table
> dynamic-dispatch gap identified below is now implemented with runtime
> validation and a safe outer fallback. W-67 is retired; Rational's temporary
> raw allocator is W-70 under #11005, and unresolved runtime candidate sets
> remain fail-closed under #10971.

> **Amendment (2026-07-14, Issue #10962):** the "Considered Designs"
> Option 3 carrier shipped as designed. The "Problem" section's implicit
> premise — that #10969's `Rational{T}` symptom is caused by cache
> round-trip identity loss — did not hold empirically: an uncached,
> freshly source-compiled Base reproduces the identical unnormalized
> output. See the implementation plan's amendment note
> (`docs/superpowers/plans/2026-07-14-constructor-self-origin-cache.md`)
> for the corrected root cause and the resulting scope decision (W-67 not
> removed, #10969 left open).

## Goal

Make Julia constructor self-family identity durable across fresh compilation,
Base-cache serialization, cache restoration, method-table cloning, and
constructor dispatch. A cached call such as `Rational{T}(x, x + x)` must select
the normalizing explicit-parametric inner constructor exactly as an uncached
compile does. The prevention must also keep bare and braced constructor calls,
same-family redefinition, runtime caller bindings, macro reconstruction, and
AoT behavior observable in tests.

## Problem

Julia distinguishes constructor methods by an implicit callable-self argument:
bare `Foo(...)` uses the `Type{Foo}` family, while `Foo{T}(...)` uses the
`Type{Foo{T}}` family. sjulia projects that argument out of `MethodSig`.
PR #10973 compensated with two `SharedCompileContext` global-index sets, but
those sets are transient. Cached Base method tables retain the method rows and
lose which rows came from bare or explicit-parametric inner constructors. The
compiler then falls back to `has_where_params()`, which cannot distinguish an
outer `Foo(x::T) where T` from an explicit inner constructor and lets
`Rational{T}` reach raw allocation.

The origin cannot be reconstructed from arity, value signatures, bounds, or
`where` count: #10959 is the counterexample where all of those projections are
identical.

## Considered Designs

### 1. Serialize the two `SharedCompileContext` index sets

This is the smallest patch, but keeps authoritative method identity outside the
method table. Every cache, clone, filter, and reconstruction path would have to
transport two correlated sets, including an invalid state where an index is
explicit-parametric but not inner. This reduces the immediate diff but does not
make the invariant structurally durable.

### 2. Add a self-family field to every `MethodSig`

This makes each row fully self-describing, but `MethodSig` is a shared dispatch
record with more than eighty construction sites, most unrelated to
constructors. It would enlarge the universal method wire and push constructor
metadata through inference-only signatures that never need it.

### 3. Store typed origin metadata inside `MethodTable` (selected)

Add a serialized, deterministic table-local map from global method index to a
typed `ConstructorSelfFamily` (`BareInner` or `ExplicitParametricInner`). An
absent entry is an ordinary method or outer constructor. This preserves the
three classes already required by #10959 without burdening every `MethodSig`.
Because the carrier is part of `MethodTable`, Base-cache serialization and
normal table cloning retain the metadata automatically.

Use a `BTreeMap`, not a `HashMap`, so serialized cache bytes are deterministic.
Expose query/filter APIs rather than direct map access. Mutation APIs must
update method rows and origin metadata transactionally.

## Architecture

### Bytecode method-table carrier

- Define `ConstructorSelfFamily` in `subset_julia_vm_bytecode::method_table`.
- Add a serde-defaulted `BTreeMap<usize, ConstructorSelfFamily>` to
  `MethodTable`.
- `add_inner_constructor_method` accepts the typed family, replaces only an
  existing inner row in the same family, removes the replaced row's origin,
  and records the new row atomically.
- `clone_for_reprojection` carries the complete map.
- `clone_with_methods_for_compile` retains only metadata whose global indices
  remain in the filtered row set.
- Query APIs answer whether a method is any inner constructor, an explicit
  inner constructor, and whether a table contains explicit inner rows.

### Compiler consumption

- Remove the two transient constructor-index sets from
  `SharedCompileContext`.
- Registration derives the typed family from
  `InnerConstructor.is_explicit_parametric` and writes it to the table.
- Explicit `Foo{T}` selection filters by table-owned
  `ExplicitParametricInner` metadata.
- Bare `Foo(...)` selection excludes only explicit-parametric inner rows.
- Dynamic caller-`where` forwarding uses the same table-owned metadata for
  both user and cached Base methods.
- Remove W-67 only after cached `Rational` normalization and the existing
  `StepRange` native-carrier regression both pass. If origin-correct dispatch
  exposes a distinct range-carrier gap, file that gap before fixing it and use
  a structural carrier solution rather than a package/type-name shortcut.

### Cache compatibility

The new serialized method-table field changes the Base cache schema. Bump
`CACHE_VERSION`, update the audited fingerprint, and keep `#[serde(default)]`
so older in-memory/test payload shapes fail or rebuild cleanly according to the
existing cache-version boundary. No heuristic reconstruction is permitted.

## Test Strategy

Follow red-green-refactor.

1. Add a real Base-cache boundary regression in the existing consolidated
   compile/cache test module: source compile, serialize, deserialize, restore,
   compile the `Rational{T}(x, x+x)` MWE, and assert `1/2`. Include an uncached
   control. This must fail before the carrier implementation.
2. Add bytecode unit tests proving origin metadata serializes, same-family
   replacement removes stale indices, different families coexist, and filtered
   clones retain only live metadata.
3. Extend the #10959 upstream-parity fixture with overload-specificity and
   binder-order cells without duplicating its existing exact-collision,
   runtime-expression, and redefinition assertions.
4. Add a focused macro fixture covering long-form and short-form reconstructed
   inner constructors.
5. Add a print-only VM/AoT constructor parity case after #10976 supplies the
   missing AoT constructor-callable layer. The 2026-07-14 design probe proved
   upstream/VM output `21, 3` while AoT raw-allocated `2, 2`; #10976 was filed
   before implementation and is a newest-first dependency of this prevention.
6. Add a constructor registration/dispatch section to CHECKLISTS.md requiring
   bare/braced calls, exact projected collisions, distinct bodies,
   last-definition-wins, runtime `T`, runtime expressions, cache round-trip,
   and explicit AoT differential evidence when AoT parity is claimed.

Keep #10968 (runtime-local `DataType`) and #10971 (runtime overload candidate
bindings) as independent owner Issues; this change must not silently claim
those runtime-dependent dispatch cases.

## Verification

- Upstream Julia and direct sjulia parity for all changed fixtures.
- Bytecode and cache-focused unit tests.
- Struct and macro fixture categories under `release-fast`.
- Fixture naming, manifest, registration, chunk-size, workaround, and Base
  cache schema audits.
- Cached `Rational` prints normalized `1/2`; existing untyped `StepRange`
  indexing remains `[10, 30]`.
- AoT differential scripts for any accepted AoT fixture, plus `test_aot.sh` if
  AoT code or accepted AoT coverage changes.
- `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, and the
  guarded full release suite before regular merge.

## Error and Compatibility Behavior

Missing origin metadata means “ordinary/outer”, never “guess from `where`”. A
cache from another schema version is rebuilt by the existing version gate.
Filtered or replaced method rows must not leave stale origin entries; debug
assertions and unit tests make that invariant explicit. Runtime expressions
and local `DataType` values remain on their existing dynamic paths until their
owner Issues are resolved.

## Success Criteria

- The real cached-Base `Rational` regression is green and #10969 closes.
- No constructor selector uses `has_where_params()` as an origin heuristic
  when typed metadata is available.
- The constructor-origin carrier survives serialization and all table clone
  paths.
- W-67 is removed only with Rational and StepRange evidence.
- #10976 is merged first and its AoT exact-collision fixture remains green.
- The prevention matrix and CHECKLISTS entry make future origin loss visible.
- PR #10974 is regularly merged and both #10974 and #10969 are closed.
