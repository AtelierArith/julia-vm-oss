# Active Workarounds in SubsetJuliaVM

This document catalogues all active workarounds in the VM codebase, along with their impact, location, and linked tracking issues. Workarounds are marked with `// Workaround: ...` comments in Rust source and `# Workaround: ...` comments in Julia source.

**Rust audit command:** `rg -n --glob '*.rs' "// Workaround:" subset_julia_vm/src/`

---

## Compile — Macro Helper Guarded AST Field Access

**File:** `subset_julia_vm/src/compile/expr/struct_.rs`

```rust
// Workaround: defer invalid macro helper field access guarded by `isa(x, QuoteNode)` (Issue #7535).
```

**Impact:** Macro helper functions may contain branches that access
`QuoteNode.value` only after an `isa(x, QuoteNode)` guard, while compile-time
specialization has inferred the macro argument as `Expr` for another path. The
compiler emits a dynamic field access instead of rejecting the helper during
macro expansion, so only actually executed invalid paths fail.

**Linked issue:** #7535

**Resolution path:** Improve macro compile-time branch/type refinement so
guarded AST helper branches compile with the narrowed type instead of needing
dynamic field fallback.

---

## VM — Typed Executable Array Store Falls Back to Interpreter

**File:** `subset_julia_vm/src/vm/executable.rs`

```rust
// Workaround: the regular VM StoreSlotArray path can fall back
// to storing arbitrary Value payloads when a statically
// array-typed slot receives a macro/runtime Expr array. The
// typed executable array stack has no equivalent fallback, so
// let the normal interpreter handle this instruction (Issue
// #7538).
```

**Impact:** Typed executable predecoding skips `StoreSlotArray` blocks that may
need the generic VM slot fallback for macro/runtime AST values. This preserves
MacroTools expansion behavior at the cost of losing the typed-loop fast path for
that instruction sequence.

**Linked issue:** #7538

**Resolution path:** Teach typed executable array slots to represent or safely
fall back for non-array `Value` payloads, then re-enable `StoreSlotArray`
predecoding.

---

## Base — `asyncmap` Sequential Approximation

**File:** `subset_julia_vm/src/julia/base/asyncmap.jl`, `subset_julia_vm/src/julia/base/mod.rs`

```rust
// Workaround: sequential approximation of asyncmap pending real Task scheduler (Issue #3500)
pub const ASYNCMAP_JL: &str = include_str!("asyncmap.jl");
```

**Impact:** `asyncmap(f, c...; ntasks, batch_size)` is implemented in pure Julia on top of the existing `Task`/`schedule`/`fetch` plumbing, but SubsetJuliaVM's `Task` runs scheduled work synchronously to completion (see `docs/vm/UNIMPLEMENTED.md` task scheduler notes). The observable result and API surface match `map` and upstream `asyncmap` for sequential workloads, but no real concurrency occurs — `ntasks` has no scheduling effect, and side-effect orderings that depend on true parallel interleaving will not be reproduced.

**Linked issue:** #3500

**Resolution path:** Replace once a real cooperative/parallel `Task` scheduler lands; the Julia-level `asyncmap` body should then start to exhibit true concurrency without further changes.

---

## AbstractAlgebra — `@attributes Type` Branch Deferred

**File:** `subset_julia_vm/packages/AbstractAlgebra/src/Attributes.jl`

```julia
# Workaround: this upstream branch generates quoted typed parameters with
# interpolated type annotations, which sjulia cannot lower yet. (Issue #7933)
```

**Impact:** The AbstractAlgebra Phase 2 gate supports the `@attributes mutable
struct ... end` form needed by `ConcreteTypes.jl`, but applying `@attributes` to
an existing type name intentionally errors until quoted typed-parameter
interpolation lowers correctly.

**Linked issue:** #7933

**Resolution path:** Restore the upstream branch once quote lowering supports
typed parameters whose annotation is an interpolated expression.

---

## AbstractAlgebra — Attribute Storage Uses Untyped Dict

**File:** `subset_julia_vm/packages/AbstractAlgebra/src/Attributes.jl`

```julia
# Workaround: upstream uses typed `Dict{...}()` constructors here, but typed
# Dict constructors with DataType parameters are not supported yet. (Issue #7934)
```

**Impact:** Attribute storage in the bundled AbstractAlgebra Phase 2 gate uses
untyped dictionaries. The early `ConcreteTypes.jl` load gate does not exercise
attribute storage behavior, so package load and struct macro expansion remain
covered while the typed dictionary constructor gap is tracked separately.

**Linked issue:** #7934

**Resolution path:** The VM gap is resolved — typed `Dict{...}()` constructors
with DataType-valued parameters work on current main (regression fixture
`dict/dict_typed_datatype_param_ctor_7934.jl`). Restoring the upstream typed
constructors in the bundled package is deferred to the AbstractAlgebra Phase 2
restoration (the surrounding attribute-storage code still depends on W-30/W-31).

---

## AbstractAlgebra — Singleton Attribute Storage Deferred

**File:** `subset_julia_vm/packages/AbstractAlgebra/src/Attributes.jl`

```julia
# Workaround: upstream indexes singleton attribute storage by the
# DataType-valued generic parameter `T`, but generic DataType Dict keys are
# not supported yet. (Issue #7940)
```

**Impact:** Attribute lookup for singleton AbstractAlgebra types returns
`nothing`, and mutating singleton attribute storage errors. The Phase 2 driver
gate only validates package load, macro expansion, aliases, and exported type
bindings; it does not rely on singleton attribute mutation.

**Linked issue:** #7940

**Resolution path:** The VM compile gap is resolved — generic `DataType` Dict
keys (`D[T]` / `D[T] = v`) compile and dispatch through the Dict get/set path
(fixture `dict/dict_generic_datatype_keys_7940.jl`). Restoring the upstream
`_singleton_attr_storage[T]` package logic additionally requires the separate
**runtime** fix for module-global `const` Dict `getindex`/`get` inside a
same-module function (#8068); until then the singleton storage stub remains.

---

## AbstractAlgebra — Attribute Mutation Deferred

**File:** `subset_julia_vm/packages/AbstractAlgebra/src/Attributes.jl`

```julia
# Workaround: upstream lazily initializes `G.__attrs` here, but generic field
# assignment to a macro-injected field is not supported yet. (Issue #7941)
```

**Impact:** Non-singleton attribute mutation errors instead of lazily creating
`G.__attrs`. The Phase 2 driver gate confirms the macro-injected field can be
lowered and reflected, but later AbstractAlgebra behavior that mutates
attributes remains deferred.

**Linked issue:** #7941

**Resolution path:** The VM gap is resolved — guarded generic field assignment
to an unknown field on an `Any`/generic receiver now compiles (deferred to a
runtime `SetFieldByName`); fixture
`struct/struct_guarded_generic_field_assign_7941.jl`. Restoring the upstream
lazy `G.__attrs = Dict()` in the bundled package is deferred to the
AbstractAlgebra Phase 2 restoration.

---

## AbstractAlgebra — UniversalRing Constructor Uses Fixed Type Parameters

**File:** `subset_julia_vm/packages/AbstractAlgebra/src/ConcreteTypes.jl`

```julia
# Workaround: upstream computes these type parameters with
# `elem_type(R)` / `elem_type(coefficient_ring(R))`, but dynamic `new{...}`
# inner constructor parameters are not supported yet. (Issue #7935)
```

**Impact:** The bundled AbstractAlgebra Phase 2 gate registers
`UniversalRing`, but its constructor uses fixed placeholder type parameters.
The early package-load gate does not instantiate `UniversalRing`; later algebra
phases must restore the upstream dynamic parameter computation before relying on
this constructor.

**Linked issue:** #7935

**Resolution path:** The VM gap is resolved — an inner constructor with computed
`new{elem_type(R), elem_type(coefficient_ring(R))}(R)` type parameters now builds
the concrete parametric type (`typeof(r).parameters == (MyElem, MyElem)`, not
`{Any}`); fixture `struct/struct_dynamic_new_type_params_7935.jl`. Restoring the
upstream constructor body in the bundled package is deferred to the
AbstractAlgebra Phase 2 restoration; it relies on the `new{}` helper functions
being resolvable in the constructor scope (module-private helpers hit the
separate ctor-body scope limitation #8069).

## AbstractAlgebra — Rational Parent Constructors Use Two-Argument Form

**File:** `subset_julia_vm/packages/AbstractAlgebra/src/julia/Rational.jl`

```julia
# Workaround: `Rational{T}(x)` can construct malformed Rational{BigInt}
# values when `T` comes from a parametric method. (Issue #8253)
```

**Impact:** The bundled AbstractAlgebra `Rationals{T}` parent uses
`Rational{T}(T(x), T(1))` for `zero`, `one`, and parent-call construction.
This preserves upstream-visible values for `QQ` while avoiding malformed
`Rational{BigInt}` values in sjulia's parametric single-argument constructor
path.

**Linked issue:** #8253

**Resolution path:** Restore the upstream single-argument constructor form once
`Rational{T}(x)` with `T` recovered from a parametric method builds a complete
`Rational{BigInt}` value.

## AbstractAlgebra — Numeric Methods Avoid Same-Module Alias Calls

**File:** `subset_julia_vm/packages/AbstractAlgebra/src/julia/Integer.jl`,
`subset_julia_vm/packages/AbstractAlgebra/src/julia/Rational.jl`

```julia
# Workaround: same-module const function aliases such as `is_zero` are not
# visible inside later method bodies in sjulia. (Issue #8254)
```

**Impact:** Internal bundled AbstractAlgebra numeric methods call `iszero`
directly instead of the upstream `is_zero` alias. The public alias remains
exported and usable; this only avoids same-module alias lookup from later method
bodies.

**Linked issue:** #8254

**Resolution path:** Restore upstream alias calls once same-module `const`
function aliases are visible inside methods compiled after the alias binding.

## AbstractAlgebra — Rational Exact Division Uses `/`

**File:** `subset_julia_vm/packages/AbstractAlgebra/src/julia/Rational.jl`

```julia
# Workaround: `//(::Rational, ::Rational)` / rational exact-division shapes
# fail in sjulia; `/` preserves the upstream-visible rational result here.
# (Issue #8255)
```

**Impact:** The bundled AbstractAlgebra MVP implements rational `divexact`
methods with `/` rather than upstream's `//` expression. The fixture-covered
results match upstream for exact rational division over `QQ`, while the Base
operator gap is tracked separately.

**Linked issue:** #8255

**Resolution path:** Restore upstream `a//b` forms once sjulia supports
rational-over-rational `//` without an internal VM error.

## AbstractAlgebra — Polynomial BigInt Accumulation Re-Coerces Any Slots

**File:** `subset_julia_vm/packages/AbstractAlgebra/src/Poly.jl`

```julia
# Workaround: adding BigInt through an `Any` array zero slot widens
# to Float64 in sjulia, so store the first product directly.
# (Issue #8262)
```

**Impact:** The dense polynomial MVP stores coefficient vectors as `Any[]`
because `T[]` with a method type variable currently degrades to `Vector{Any}`.
For `ZZ` polynomials, multiplication and exact division re-coerce accumulated
coefficient slots through the base ring and rebuild remainder vectors with
`push!` to avoid `BigInt` widening to `Float64`.

**Linked issue:** #8262

**Resolution path:** Restore straightforward `coeffs[i] += product` and
in-place remainder updates once `BigInt` arithmetic through `Any` array slots
preserves `BigInt`.

## AbstractAlgebra — Fraction Field Uses Constructor Helper

**File:** `subset_julia_vm/packages/AbstractAlgebra/src/FractionResidue.jl`,
`subset_julia_vm/tests/fixtures/packages/abstract_algebra_fraction_residue_7491.jl`

```julia
# Workaround: callable fraction-field parent dispatch fails for
# `F(num, den)` in sjulia, so internal arithmetic routes through `_frac_make`.
# (Issue #8264)
```

**Impact:** The bundled AbstractAlgebra fraction-field MVP exposes the
upstream-shaped callable parent methods, but internal arithmetic and fixtures
use `_frac_make(F, num, den)` until sjulia dispatches `F(num, den)` correctly.

**Linked issue:** #8264

**Resolution path:** Replace helper calls with normal parent calls after
callable `SimpleFracField` dispatch works for polynomial numerator/denominator
arguments.

---

## Tests — Distributions Fixtures Avoid Global Keyword Defaults

**Files:**
- `subset_julia_vm/tests/fixtures/distributions/distributions_fit_suffstats_7326.jl`
- `subset_julia_vm/tests/fixtures/distributions/distributions_test_dists_7327.jl`

```julia
# Workaround: omitted keyword defaults that reference globals can evaluate as 0
# in sjulia (Issue #7774). Keep tolerance helpers positional-only here.
```

**Impact:** Distributions fixtures use positional tolerance helpers instead of
`close(a, b; atol=tol)`. Upstream Julia evaluates the omitted keyword default
from the global `tol`, but sjulia currently observes `0` for that omitted
default, making exact-equality comparisons fail.

**Linked issue:** #7774

**Resolution path:** Once omitted keyword defaults evaluate global bindings
correctly, restore the idiomatic keyword tolerance helper.

---

## Tests — Distributions PoissonBinomial Fixture Avoids Tuple Vector Equality

**File:** `subset_julia_vm/tests/fixtures/distributions/distributions_discrete_expansion_7331.jl`

```julia
# Workaround: tuple equality with equal Vector elements returns false in
# sjulia, so compare the vector parameter directly instead. (Issue #7803)
return params(d)[1] == [0.2, 0.5, 0.8] &&
```

**Impact:** The #7331 parity fixture still checks `PoissonBinomial` parameters,
but compares the vector inside `params(d)` directly instead of using the
upstream-style `params(d) == ([...],)` tuple comparison. sjulia currently
returns `false` for tuples that contain structurally equal vectors.

**Linked issue:** #7803

**Resolution path:** Restore the direct tuple comparison once tuple equality
recurses through array equality like upstream Julia.

---

## MacroTools — `combinearg` Builds Typed Argument Exprs Explicitly

**File:** `subset_julia_vm/packages/MacroTools/src/utils.jl`

```julia
# Workaround: quoted type-annotation syntax with interpolation currently
# lowers as a runtime typeassert instead of constructing Expr(:(::), ...).
# Build the syntax tree explicitly. (Issue #7628)
```

**Impact:** `combinearg(:x, :Any, false, nothing)` constructs
`Expr(:(::), :x, :Any)` directly instead of relying on `:($arg_name::$arg_type)`.

**Linked issue:** #7628

**Resolution path:** Preserve quoted `::` interpolation as an Expr constructor
when lowering quoted syntax from macro/package code.

---

## MacroTools — `shortdef1` Builds Typed Branch Exprs Explicitly

**File:** `subset_julia_vm/packages/MacroTools/src/utils.jl`

```julia
# Workaround: avoid nested @q splatted interpolation in shortdef patterns
# while preserving the upstream Expr shape. (Issue #7541)
```

**Impact:** Typed `shortdef1` branches construct the same assignment Exprs
directly instead of relying on nested `@q` splatted interpolation while loading
MacroTools helpers.

**Linked issue:** #7541

**Resolution path:** Preserve nested MacroTools `@q` splatted interpolation in
function-local pattern branches without early macro-local evaluation.

---

## MacroTools — `resyntax` Builds Field Assignment Exprs Explicitly

**File:** `subset_julia_vm/packages/MacroTools/src/utils.jl`

```julia
# Workaround: construct field-assignment syntax explicitly because
# macro-result lowering cannot yet round-trip quoted `x.f = v`
# assignment targets. (Issue #7630)
```

**Impact:** `resyntax` returns explicit `Expr(:(=), Expr(:., ...), ...)` and
`Expr(:(+=), Expr(:., ...), ...)` trees instead of quoted field assignment
syntax.

**Linked issue:** #7630

**Resolution path:** Support quoted field assignment targets in macro-result
lowering, then restore the upstream `:($x.$f = $v)` / `:($x.$f += $v)` forms.

---

## MacroTools Fixtures — Macro-Generated Function Definitions Deferred

**File:** `subset_julia_vm/tests/fixtures/macrotools/upstream/split.jl`, `subset_julia_vm/tests/fixtures/macrotools/upstream/utils.jl`

```julia
# Workaround: full @splitcombine support needs macro-generated
# Expr(:function, ...) definitions to lower back to function definitions.
# (Issue #7634)

# Workaround: @qq function definitions expand to Expr(:function, ...), which
# sjulia cannot lower back into a function definition yet. (Issue #7634)
```

**Impact:** The fixture covers direct `splitarg`, `combinearg`, and `combinedef`
smoke behavior, while the upstream `@splitcombine` function-definition tests are
deferred. The upstream `@qq function fff() ... end` line-number test in
`utils.jl` is also deferred for the same macro-generated function-definition
lowering gap.

**Linked issue:** #7634

**Resolution path:** Lower macro-generated `Expr(:function, ...)` values back
into function definitions, including signatures, defaults, where clauses, and
anonymous-function forms, then restore the full upstream split fixture.

---

## MacroTools — `@destruct` Reads Structural Patterns Directly

**File:** `subset_julia_vm/packages/MacroTools/src/examples/destruct.jl`

```julia
# Workaround: sjulia does not yet capture MacroTools @destruct array/ref patterns
# through the upstream @match surface, so mirror the small structural cases here.
# (Issue #7636)
```

**Impact:** `@destruct` recognizes the upstream array/ref/field pattern cases
used by `destruct.jl` without relying on the broader `@match` pattern surface
that currently misses those shapes in sjulia.

**Linked issue:** #7636

**Resolution path:** Fix MacroTools pattern capture for array/ref/field syntax
under sjulia, then restore the upstream `@match`-only implementation.

---

## MacroTools — Atomic `destruct_key` Avoids Captured Callable Closure

**File:** `subset_julia_vm/packages/MacroTools/src/examples/destruct.jl`

```julia
# Workaround: avoid a postwalk closure that calls a captured callable
# argument until sjulia supports that MacroTools helper pattern. (Issue #7637)
```

**Impact:** Atomic destructuring keys such as `:a` call `getm(val, pat)`
directly instead of routing through `atoms(i -> getm(val, i), pat)`, avoiding a
closure/callable capture gap while preserving the returned AST.

**Linked issue:** #7637

**Resolution path:** Support closures that call captured callable arguments in
MacroTools helper contexts, then route atoms through the upstream `postwalk`
path again.

---

## MacroTools — `unresolve` Inlines Function Leaf Dispatch

**File:** `subset_julia_vm/packages/MacroTools/src/utils.jl`

```julia
# Workaround: inline the Function leaf branch because passing the generic
# `unresolve1` function through prewalk dispatches to the catch-all method for
# function values in sjulia. (Issue #7711)
unresolve(ex) = prewalk(x -> x isa Function ? nameof(x) : unresolve1(x), ex)
```

**Impact:** `prettify(:($sin(2)))` and `prettify(:($cos(x)))` normalize
interpolated function values to `:sin` / `:cos` without depending on sjulia's
higher-order dispatch for a passed generic `unresolve1` function value.

**Linked issue:** #7711

**Resolution path:** Fix higher-order dispatch for passed generic function
values so `prewalk(unresolve1, ex)` re-dispatches to `unresolve1(::Function)`,
then restore the upstream form.

---

## OrdinaryDiffEq — `Tsit5` + `solve(::ODEProblem, ::Tsit5)` live in SciMLBase

**File:** `subset_julia_vm/packages/SciMLBase/src/SciMLBase.jl` (`struct Tsit5`,
`solve(::ODEProblem, ::Tsit5)`), re-exported by
`subset_julia_vm/packages/OrdinaryDiffEq/src/OrdinaryDiffEq.jl` (`import SciMLBase: Tsit5`).

```julia
# Workaround: register the Tsit5 alg dispatch ON SciMLBase.solve by defining both
# the `Tsit5` type and `solve(::ODEProblem, ::Tsit5)` inside SciMLBase, then
# re-exporting `Tsit5` from the OrdinaryDiffEq facade. (Issue #8052)
```

**Impact:** Upstream layering would have the OrdinaryDiffEq solver package
*extend* `SciMLBase.solve` (`function SciMLBase.solve(prob, alg::Tsit5; …)`),
keeping `Tsit5` in OrdinaryDiffEq. sjulia cannot extend another module's function
(#8052: `function SciMLBase.solve(...)` → lowering "missing function name";
`import SciMLBase: solve; function solve(...)` → a separate `OrdinaryDiffEq.solve`
that the qualified `SciMLBase.solve(prob, Tsit5())` never sees). So both the alg
type and its `solve` method are defined together in SciMLBase and the facade only
re-exports the type. This keeps a single `solve` method table — `solve(prob,
Tsit5())` (via the OrdinaryDiffEq forwarder) and the qualified `SciMLBase.solve(prob,
Tsit5())` now dispatch identically, fixing the PR #8050 review regression — at the
cost of qualified `OrdinaryDiffEq.Tsit5` access (#8053; only the package-skeleton
fixture used it). `Tsit5` is imported, not `const`-aliased, so the constructor
works (avoids #8049). `SciMLBase.Tsit5` does not exist upstream, but the MVP
already keeps the `_tsit5_solve` stepper in SciMLBase, so the alg token sits with
its solver.

**Linked issue:** #8052 (root cause), #8053 (qualified re-export access casualty);
PR #8050 review.

**Resolution path:** Implement cross-module function extension (#8052) so
OrdinaryDiffEq can `function SciMLBase.solve(prob, alg::Tsit5; …)` directly; then
move `Tsit5` back to OrdinaryDiffEq and drop the SciMLBase re-export.

---

## JSXGraph — `board` Bounding-Box Array Typed as `Float64[]` (W-39)

**File:** `packages/JSXGraph/src/api.jl`
**Issue:** #8072
**Symptom:** `TypeError: BoundsError: Memory access error: TypeError: Cannot store F64 in I64 array`
when the Apollonian Gasket sample (and any code that calls `board()` with default integer limits first,
then calls it again with Float64 limits) is run.

**Root cause:** The VM specializes array literals based on the types seen at the first call. When
`board()` is called with the integer defaults `xlim=(-5, 5)`, the compiler emits `NewMemory(I64, 4)`
for `bb = [xlim[1], ylim[2], xlim[2], ylim[1]]`. A subsequent call with `xlim=(-3.15, 3.15)` reuses
the cached bytecode and tries `MemorySet(bb, F64(-3.15))` on a `Memory{Int64}`, which raises the error.
(Issue #8072 tracks the underlying VM type-specialization bug.)

**Workaround:** `bb = Float64[xlim[1], ylim[2], xlim[2], ylim[1]]` forces the backing memory to
`Memory{Float64}` regardless of what `xlim` contains; integer values are promoted automatically and
Float64 values are always accepted.

**Resolution path:** Fix the VM compiler to either widen array-literal element types when they derive
from function parameters, or invalidate the function bytecode cache when actual argument types differ
from the types seen at first compilation (Issue #8072).

---

> **W-40 (Issue #8078) — RESOLVED.** `HagerZhang(; alphamax = Inf, ...)` is
> restored to the upstream-faithful keyword default, and the constructor body
> uses `Float64(alphamax)` again. `Optim.BFGS()` was failing to converge (it took
> the un-line-searched fixed step every iteration) because the Hager-Zhang line
> search's `alphamax` field was `0.0` instead of `Inf`, clamping the step to 0.
> The root cause was that a bare `Inf`/`NaN` keyword *default* — a Base global
> constant the compiler emits as a float literal in expression position, but not
> a bound runtime global — fell through both kwarg-default evaluators
> (`compile::utils::eval_literal_default` and `vm::exec::call::value_from_bound_name`)
> to the `Value::I64(0)` fallback. A shared `float_special_constant_value` resolver
> now maps the `Inf`/`NaN`/`Inf32`/`Inf16`/`Inf64`/`NaN*` family (and `pi`/`ℯ`) in
> both, with bound names still taking precedence; `infer_default_type` mirrors the
> precise float type (and recurses through unary `-`, so a `-Inf` `@kwdef` field
> no longer mis-typed its inner constructor dispatch slot as `Int64`). Regression:
> `kwargs::kwargs_inf_nan_default_8078`, `optim::` (BFGS without the sentinel).

> **W-41 (Issue #8079) — RESOLVED.** `iterfinitemax = ceil(Int, -log2(eps(Float64)))`
> is restored to the upstream form. The spurious `StackOverflowError` was a
> qualified-call resolution bug: NaNMath's `log2` (imported into the line search
> via `using NaNMath`) shadowed `Base.log2` in the shared short-name method table,
> so the shadow's own `Base.log2(float(x))` re-dispatched to the shadow and
> self-recursed (NaNMath.log2 → Base.log2 → NaNMath.log2 → …). The compiler now
> preserves a clobbered base method under a `Base.<name>` table so the explicit
> qualified call reaches the base implementation. See the Resolved Workarounds
> table and `tests/fixtures/modules/module_qualified_base_shadow_8079.jl`.

---

## OrdinaryDiffEq — Cross-Module `_copy_state` Dispatch Falls Through to Generic (W-43)

**File:** `packages/OrdinaryDiffEq/src/OrdinaryDiffEq.jl`
**Issue:** #8104 (W-43)
**Symptom (W-43):** All entries in `sol.u` aliased to the final ODE state after `solve(prob, Tsit5())`. `sol.u[1]` showed the final state value instead of the initial condition `prob.u0`, causing Lorenz attractor and pendulum plots to render incorrectly on iOS.

**Root cause (W-43):** From within the `OrdinaryDiffEq` module, calling the qualified `SciMLBase._copy_state(u::AbstractVector)` dispatches to the generic `_copy_state(u) = u` method (returns the same reference) instead of the `AbstractVector`-specific method `_copy_state(u::AbstractVector) = copy(u)`. This is a cross-module qualified call dispatch bug in sjulia (Issue #8104): type-annotated methods defined in a foreign module are not resolved when the call is qualified `ModName.func(arg)` from another module context.

**Workaround (W-43):** The `SciMLBase._tsit5_solve` override in `OrdinaryDiffEq.jl` replaces every `SciMLBase._copy_state(u)` call with an inline `ismutable(u) ? copy(u) : u` expression, which correctly copies mutable arrays (standard `Vector`) and aliases immutable states (`SVector`, scalars) without going through the broken dispatch path.

**Resolution path:** Fix sjulia's cross-module qualified call dispatch (Issue #8104) so that `SciMLBase._copy_state(u::AbstractVector)` wins over the generic `_copy_state(u)` when called as `SciMLBase._copy_state(some_vector)` from OrdinaryDiffEq context; then restore the `SciMLBase._copy_state` calls.

---

## AbstractAlgebra — Dense Matrix BigInt Storage Uses `Any` Slots (W-49)

**File:** `packages/AbstractAlgebra/src/Matrix.jl`
**Issue:** #8266 (W-49)

```julia
# Workaround: typed `Vector{BigInt}` / `Matrix{BigInt}` stores read back
# as Float64 in sjulia, so dense matrices use flat Any storage and coerce
# every value at the package boundary. (Issue #8266)
```

**Impact:** The Phase 6 dense matrix MVP stores entries in a flat `Any` vector
instead of upstream-style typed `Matrix{T}` storage. Public matrix construction,
indexing, arithmetic, determinant, trace, rank, and display helpers coerce
through the matrix base ring at package boundaries, so fixture-visible `ZZ`
entries still read back as `BigInt`.

**Resolution path:** Fix typed `BigInt` array storage in sjulia so
`Vector{BigInt}` and `Matrix{BigInt}` preserve assigned `BigInt` values. Then
restore dense matrix storage to typed arrays following upstream
`MatSpaceElem{T}` shape.

---

## Summary Table

| ID | Category | File | Impact | Linked Issue |
|----|----------|------|--------|--------------|
| W-43 | OrdinaryDiffEq | `packages/OrdinaryDiffEq/src/OrdinaryDiffEq.jl` | `ismutable(u) ? copy(u) : u` inline instead of `SciMLBase._copy_state(u)` because qualified cross-module dispatch fails to select the `AbstractVector` method (all `sol.u` entries aliased to final state) | #8104 |
| W-39 | JSXGraph | `packages/JSXGraph/src/api.jl` | `board` bounding-box array typed explicitly as `Float64[]` to prevent `Memory{Int64}` specialization from the integer defaults colliding with Float64 call-site args | #8072 |
| W-35 | OrdinaryDiffEq | `packages/SciMLBase/src/SciMLBase.jl`, `packages/OrdinaryDiffEq/src/OrdinaryDiffEq.jl` | `Tsit5` + `solve(::ODEProblem, ::Tsit5)` live in SciMLBase (facade re-exports `Tsit5`) so the alg registers on `SciMLBase.solve`, since sjulia cannot extend another module's function | #8052 |
| W-06 | Base | `julia/base/asyncmap.jl`, `julia/base/mod.rs` | `asyncmap` runs sequentially (`ntasks` is a no-op) | #3500 |
| W-13 | AbstractAlgebra | `packages/AbstractAlgebra/src/Attributes.jl` | `@attributes Type` branch errors until quoted interpolated typed parameters lower | #7933 |
| W-28 | AbstractAlgebra | `packages/AbstractAlgebra/src/Attributes.jl` | Attribute storage uses untyped `Dict()` until typed `Dict{...}` constructors with DataType parameters work | #7934 |
| W-30 | AbstractAlgebra | `packages/AbstractAlgebra/src/Attributes.jl` | Singleton attribute storage is disabled until `Dict` supports generic DataType keys | #7940 |
| W-31 | AbstractAlgebra | `packages/AbstractAlgebra/src/Attributes.jl` | Attribute mutation errors until guarded assignment to macro-injected fields compiles | #7941 |
| W-29 | AbstractAlgebra | `packages/AbstractAlgebra/src/ConcreteTypes.jl` | `UniversalRing` constructor uses fixed placeholder type parameters until dynamic `new{...}` parameters work | #7935 |
| W-44 | AbstractAlgebra | `packages/AbstractAlgebra/src/julia/Rational.jl` | `Rationals{T}` parent constructors use `Rational{T}(T(x), T(1))` until parametric `Rational{T}(x)` constructs complete `Rational{BigInt}` values | #8253 |
| W-45 | AbstractAlgebra | `packages/AbstractAlgebra/src/julia/Integer.jl`, `packages/AbstractAlgebra/src/julia/Rational.jl` | Internal numeric methods call `iszero` directly until same-module const function aliases are visible inside later method bodies | #8254 |
| W-46 | AbstractAlgebra | `packages/AbstractAlgebra/src/julia/Rational.jl` | Rational `divexact` methods use `/` until rational-over-rational `//` works in sjulia | #8255 |
| W-47 | AbstractAlgebra | `packages/AbstractAlgebra/src/Poly.jl` | Dense polynomial BigInt accumulation re-coerces/rebuilds `Any` coefficient slots until BigInt arithmetic through `Any` slots preserves BigInt | #8262 |
| W-48 | AbstractAlgebra | `packages/AbstractAlgebra/src/FractionResidue.jl`, `tests/fixtures/packages/abstract_algebra_fraction_residue_7491.jl` | Fraction-field arithmetic uses `_frac_make(F, num, den)` until callable `F(num, den)` dispatch works for polynomial arguments | #8264 |
| W-49 | AbstractAlgebra | `packages/AbstractAlgebra/src/Matrix.jl` | Dense matrix entries use flat `Any` storage with base-ring coercion until typed `Vector{BigInt}` / `Matrix{BigInt}` storage preserves BigInt values | #8266 |
| W-14 | MacroTools | `packages/MacroTools/src/utils.jl` | `combinearg` builds typed-argument Exprs explicitly until quoted `::` interpolation is preserved | #7628 |
| W-16 | MacroTools | `packages/MacroTools/src/utils.jl` | `resyntax` builds field assignment Exprs explicitly until quoted field assignment targets round-trip | #7630 |
| W-17 | Tests | `tests/fixtures/macrotools/upstream/split.jl`, `tests/fixtures/macrotools/upstream/utils.jl` | Full `@splitcombine` and `@qq function` tests are deferred until macro-generated `Expr(:function, ...)` lowers to definitions | #7634 |
| W-18 | MacroTools | `packages/MacroTools/src/examples/destruct.jl` | `@destruct` mirrors structural array/ref/field pattern cases until `@match` captures them | #7636 |
| W-19 | MacroTools | `packages/MacroTools/src/examples/destruct.jl` | Atomic destruct keys avoid captured-callable postwalk closure | #7637 |
| W-22 | MacroTools | `packages/MacroTools/src/utils.jl` | `shortdef1` builds typed branch Exprs directly until nested `@q` splatted interpolation preserves function locals | #7541 |
| W-25 | MacroTools | `packages/MacroTools/src/utils.jl` | `unresolve` inlines Function leaf dispatch until HOF generic-function redispatch works | #7711 |
| W-27 | Tests | `tests/fixtures/distributions/distributions_discrete_expansion_7331.jl` | PoissonBinomial parameter fixture compares the vector directly until tuple equality recurses through Vector equality | #7803 |
| W-36 | Base/Broadcast | `julia/base/broadcast.jl` | Phase 1-2 non-parametric `BroadcastStyle`/`Broadcasted`: concrete `DefaultArrayStyleN` types instead of `DefaultArrayStyle{N}`, `bc_args` field name (avoids `Expr.args` collision), simplified `broadcast_shape`/`combine_axes`, `Tuple{}` dispatch | #2531, #2534, #2535, #2536, #2546, #2523 |
| W-37 | Base | `julia/base/array.jl` | `wrap(::Type{Array}, m::Array, dims)` guards against compile-time inference projecting a runtime-`T` `Memory{T}(n)` as `Array` while compiling `similar(a, T, dims...)` | #4018 |
| W-38 | Base | `julia/base/iterators.jl` | `flatmap` uses a `FlatMap` struct instead of upstream `flatten(map(f, c...))` due to a `map` transposition bug | #2119 |

These three rows (W-36–W-38) were catalogued by the #7818 drift audit: their
`# Workaround:` comments carry Issue links but were missing from this document.

**No dedicated tracking issue (pre-existing simplifications, inventoried for
completeness by #7818):**

| Location | Workaround |
|----------|------------|
| `julia/base/broadcast.jl` (`_make_leaf_selector`, `_broadcast_apply`) | Captured variables cannot be used as direct call targets in closures, so fusion routes through a trampoline instead of a parametric `Pick{N}` callable struct |
| `julia/base/iterators.jl` (`peel`) | Guards against calling `peel` on an empty iterator (VM type-inference issue for the empty case) |
| `julia/base/macros.jl` (`@eval`) | `@eval ex` expands to `esc(ex)` (inline expansion) instead of evaluating the expression in module scope |
| `tests/stdlib/test_InteractiveUtils.jl`, `tests/fixtures/packages/plots_scatter_bang.jl` | Test-side spelling guards (`@test isa(...)` assigned-first; literal vectors vs `collect`) |

---

## Resolved Workarounds

| PR | Issue | Description |
|----|-------|-------------|
| pending | #8435 | `retry(f; check=...)` now uses upstream-style `rethrow()` again when `check` rejects a retry. Direct `rethrow()` calls compile to the VM `RethrowCurrent` primitive before the documented Base stubs can win method dispatch, and catch blocks keep the caught exception available after `ClearError` so nested catches propagate to the outer handler. Regression: `exceptions_rethrow_nested_8435`, existing `exceptions/rethrow.jl`, and `retry_8371`. |
| pending | #8313 | Imported bare-name dispatch for exported parametric inner constructors now tries the visible qualified module constructor chain (`M.T`) before unqualified dispatch. The milestone fixture `milestone55_imported_parametric_inner_constructor_8313` covers `using .M; Perm([1,2,3])` selecting the imported inner constructor. |
| pending | #5005 | `isdefined` (W-07) restored to upstream two-method form: `isdefined(m::Module, ::Symbol)` and `isdefined(x, ::Symbol)` are now separate methods instead of one method branching on `isa(x, Module)`. A `::Module`-typed parameter now wins dispatch specificity over an untyped parameter, so the dedicated module method is selected (verified: `g(m::Module,::Symbol)` beats `g(x,::Symbol)` for `g(Base, :sin)`). Retired under the #7812 / #7816 audit. Regression: `reflection::` category. |
| pending | #5611 | `ReshapedArray` 2D five-parameter SubArray reshape (W-08) restored to the generic path: the focused `reshape(::SubArray{Int64,1,Vector{Int64},Tuple{UnitRange{Int64}},true}, dims)` constructor branch and the `ndims(::ReshapedArray{T,2,P,MI}) = 2` value-parameter special case were removed; 2D `reshape(view(...), 2, 2)` now routes through the generic `reshape(a::SubArray{T,N,P,I,L}, dims)` → `_reshapedarray_checked`, and `ndims` reads `N` from the value parameter directly (after #7728 parametric-template registration fixed the parent type-parameter and value-parameter reads). Retired under #7812 / #7816. Regression: `subarray::`, `views::`, `types::` (incl. `wrapper_array_core_subtype_gate_5615`). |
| pending | #6661 | `Dict` typed storage helper (W-09) restored to anonymous `::Type{K}, ::Type{V}` parameters: `_new_dict_kv(::Type{K}, ::Type{V}, n) where {K,V}` no longer needs distinctly named (`_key_type`/`_val_type`) arguments — repeated anonymous typed parameters now keep independent `where` bindings (verified: `h(::Type{K}, ::Type{V}, n)` returns the correct `(K, V, n)`). Retired under #7812 / #7816. Regression: `dict::` category. |
| pending | #7741 | MacroTools `gatherwheres` (W-26) restored to the upstream `(f2, (params1..., params2...))` tuple-literal splat instead of `tuple(params1..., params2...)`: splatted elements now splice rather than nest in tuple-literal lowering (verified: `(params1..., params2...)` → `(:T, :U)`). Retired under #7812 / #7815. Regression: `macrotools::` category (`gatherwheres` smoke). |
| pending | #2425 | Compile-time "dynamic call returns `Any`" workaround (W-03) is gone: `compile/expr/call/mod.rs` no longer carries the `// Workaround: return Any ... (Issue #2425)` comment (the dynamic-call return-type path was reworked in earlier dispatch work). Stale WORKAROUNDS.md entry removed under the #7812 / #7818 drift audit. |
| pending | #7266 | `Categorical(k::Integer)` workaround already retired in code (`packages/Distributions/src/univariate/discrete.jl` uses the upstream `Categorical([1.0/k for _ in 1:k])` form, routing through `Categorical(p::AbstractVector{<:Real})`); the stale WORKAROUNDS.md section was removed under the #7812 / #7818 drift audit. Regression: `distributions::` category. |
| pending | #8080 | NLSolversBase finite-difference gradient (W-42) restored to the upstream-faithful closure factory: `_central_difference_gradient(f)` again returns a closure capturing the objective `f`, built from the `OnceDifferentiable(f, x0)` constructor body, replacing the non-capturing `_central_diff_gradient!(G, obj.f, x)`. The captured-variable misresolution behind the bug — an objective bound to a variable literally named `f` threaded through `optimize(f, ...)` (whose objective parameter is also `f`) — was already fixed on main by the nested-closure capture work (#7600/#7618/#7759) merged before the BFGS feature commit; bisecting to that commit with the closure factory restored reproduces no error, so no further VM change was needed. Regression: `closures::closures_closure_factory_name_collision_8080`, `optim::optim_objective_named_f`, `optim::optim_bfgs_rosenbrock` (finite-difference subtest). |
| pending | #8042 | Optim NelderMead stopping objective (W-34) restored from the pure-arithmetic Newton `_sqrt` to the builtin `sqrt`: a bare/`Base.sqrt` call on an `Any`-typed `Float64` no longer dispatches to a foreign module's `sqrt(::Any)` (e.g. `NaNMath.sqrt`) that was merged into the global `sqrt` table and recursed to a stack overflow. `compile_sqrt` now excludes single-segment `"<Module>.sqrt"` (Module≠Base) methods from the builtin-backed candidate set and never falls through to generic dispatch for the `Any` case (Issue #8042 fixed). Regression: `module_base_sqrt_foreign_shadow_8042`, `optim_nelder_mead_mvp`. |
| pending | #8078 | LineSearches `HagerZhang.alphamax` (W-40) restored to the upstream `alphamax = Inf` keyword default + `Float64(alphamax)` constructor body. A bare `Inf`/`NaN` keyword *default* resolved to `0` because the constant — emitted as a float literal in expression position but not bound as a runtime global — fell through both kwarg-default evaluators (`eval_literal_default`, `value_from_bound_name`) to the `Value::I64(0)` fallback, so `alphamax` became `0.0` and clamped the BFGS line-search step to 0. A shared `float_special_constant_value` resolver now maps the `Inf`/`NaN`/`Inf32`/`Inf16`/`Inf64`/`NaN*` family (and `pi`/`ℯ`) in both evaluators (bound names still win), and `infer_default_type` mirrors the precise float type and recurses through unary `-` (so a `-Inf` `@kwdef` field no longer mis-types its inner-constructor dispatch slot as `Int64`). Regression: `kwargs_inf_nan_default_8078`, `optim::` (BFGS without the sentinel). |
| pending | #8079 | LineSearches `iterfinitemax` (W-41) restored to the upstream `ceil(Int, -log2(eps(Float64)))`. The spurious `StackOverflowError` was the general form of the W-34/#8042 `sqrt` bug: a module's own `log2`/`log10` (NaNMath, imported into the line search) shadows the same-signature `Base.log2` in the shared short-name method table (`add_method` dedups by signature), so the shadow's own `Base.log2(float(x))` qualified call re-dispatched to the shadow and self-recursed (NaNMath.log2 → Base.log2 → NaNMath.log2 → …). Unlike #8042 (`sqrt` has a builtin, fixed at the `compile_sqrt` candidate set), `log2`/`log10` are pure-Julia Base functions with no builtin, so the fix is general: `build_method_tables` snapshots a base method into a `Base.<name>` table the moment a user shadow clobbers it, and `compile_module_call` routes the explicit `Base.<name>(...)` call through that table. Only clobbering shadows of single-untyped-method base functions allocate a snapshot (a typed `log(::Float64)` base method is untouched by an untyped `log(::Any)` shadow). Regression: `modules_module_qualified_base_shadow_8079`, `optim::` (BFGS without the constant). |
| pending | #8025 | LinearAlgebra `inv` (W-33) restored to the typed `inv(A::AbstractMatrix)` builtin-forwarder, and the bundled Symbolics package's `inv(A::AbstractMatrix{<:Num})` now wins runtime dispatch directly: parameterized matrix types are ranked more specific than bare `AbstractMatrix` (Issue #8025 fixed by resolving the user-struct array element type for dispatch). Regression: `packages_symbolics_linear_algebra`. |
| pending | #8019 | Symbolic matmul/det/inv (W-32) restored from the fully-qualified `AbstractMatrix{<:Symbolics.Num}` bound to the bare `AbstractMatrix{<:Num}`: a `{<:Num}` bound written with the imported alias now matches a `Matrix{Symbolics.Num}` argument (Issue #8019 fixed). Regression: `packages_symbolics_matmul`, `packages_symbolics_linear_algebra`. |
| pending | #7958 | Named field access `w.x` on a module-qualified parametric inner-constructor instance (`Mod.Wrapped(41)` → `new{T}(...)`) now resolves the field table. `GetFieldByName` falls back to the compile-context `parametric_structs` schema keyed by the struct's base name when the instantiation is not in runtime `struct_defs` (its `type_id` fell back to 0). The `module_qualified_parametric_inner_constructor_7955.jl` fixture restores `@test w.x == 42` instead of only `getfield(w, 1)`. Regression: `module_qualified_parametric_inner_field_access_7958`. |
| pending | #7728 | StaticArray (W-23) restored to upstream `StaticArray{S,T,N} <: AbstractArray{T,N}` in `packages/StaticArraysCore/src/types.jl` and `packages/StaticArrays/src/abstractarray.jl`. The value-parameter parent chain now preserves the `AbstractArray{T,N}` subtype edge: `build_struct_hierarchy_from_program` registers parametric struct templates before their monomorphized instances so the family entry keeps its type-parameter NAMES (a concrete instance had clobbered them with an empty list), and `substitute_parent_name` recurses into nested parent args (`Tuple{N}`). `SVector{3,Int64}(1,2,3) isa AbstractArray{Int64,1}` is now true (Issue #7819). Regression: `static_arrays_abstractarray_subtype_7819`, `types_value_param_abstractarray_parent_chain_7819`. |
| pending | #5776 | `namedtuple_merge_5687.jl` restored upstream empty NamedTuple forms `(; )` and `NamedTuple() == (; )` (Issue #7814). |
| pending | #7741 | `partial_parametric_constructors_7734.jl` restored `(A, B, xs...)` tuple-literal splat in constructor body (Issue #7814). |
| pending | #7196 | Barnsley Fern iOS/mobile samples already use `[a b; c d]` matrix literals; stale WORKAROUNDS.md entry removed (Issue #7814). |
| pending | #7632 | Pure-Julia `Dict{K,V}` StructRef values now match bare `::Dict` annotations after `Value::Dict` carrier removal, so MacroTools `combinedef` restored the upstream `combinedef(dict::Dict)` signature instead of carrying an untyped workaround. |
| pending | #7743 | Dynamic field access on Any-typed `GlobalRef` now routes `.mod` and `.name` through the VM's dedicated GlobalRef projection, so MacroTools `rmdocs` restored the upstream `m.mod == Core && m.name == Symbol("@doc")` predicate instead of comparing whole `GlobalRef` values. |
| pending | #7683/#7727/#7730 | `eval` now executes `Expr(:try, ...)` basics, preserves else branch values, and unwinds caught eval frames before caller stores, so `tests/fixtures/macrotools/upstream/flatten_try.jl` is restored to the upstream MacroTools v0.5.16 eval coverage instead of carrying W-21. |
| pending | #7647 | MacroTools upstream `utils.jl` v0.5.16 checks are restored: typed/where `isdef`, block/try `flatten`, animals ordering, and `@qq` line metadata all pass under sjulia. |
| pending | #7643 | `CallDynamicBinaryBoth` now carries user-written binary methods into runtime signature resolution, so infix `==` after macro-generated nested block assignment reaches `==(::S, ::S)` like `==(s, S(...))`. |
| pending | #7646 | `MacroTools.animals` now resolves through module-qualified lookup as the package data `Vector{Symbol}` constant instead of the `MacroTools.animals` function binding. |
| pending | #7574 | `LinearAlgebra.LAPACK` now uses the upstream-style `import ..LinearAlgebra: inv, lu, LU`; nested module relative named imports resolve parent and sibling module paths. |
| pending | #7736 | `SMatrix` 2D indexing now uses the direct value type parameter arithmetic form `x.data[(i - 1) * N + j]`; binary compilation routes `where` type-parameter operands through runtime dispatch so integer value parameters materialize from the callee frame. |
| pending | #7577 | `LinearAlgebra.sylvester` now uses the upstream-style `-_colvec(C)` spelling because array unary minus dispatch is supported. |
| pending | #3342 | AoT no longer maps Julia subtype (`<:`) to numeric less-than. `BinaryOp::Subtype` now maps to a dedicated `AotBinOp::Subtype` and codegen rejects it as an unsupported type-relation operation instead of emitting wrong Rust. |
| pending | #7021 | VM runtime dispatch now preserves keyword payloads for ambiguous single-arg overload sets and dynamic no-method fallback paths. Plots no longer needs the single untyped `plot(y)` workaround and uses typed `plot(y::Vector)` / `plot(y::Number)` methods again. |
| pending | #3343 | AoT `missing` literals now have `AotExpr::LitMissing`, convert from Core IR `Literal::Missing`, verify/optimize as a literal, and emit `Value::Missing` in generated Rust. |
| #6524, #6525, #6529 | #6512, #6597 | Runtime `::Function` matching routes singleton function type names (`typeof(+)`) through `CoreSubtypeEngine` instead of the legacy exact-name workaround (`JuliaType::Function => runtime_type == param_type.name()`). Callable-value n-ary `+`/`*` folds use the shared narrow-integer modular wrap protocol; the f6adade84 (#6529) `typed_dispatch_signature_is_broad_any` guard counts `Function`-typed slots as broad so empty narrow-int / Bool reductions stay on the type-specialized Base method instead of the broad `reduce(op::Function, itr)` catch-all. Issue #6597 re-evaluated and confirmed the carve-out is fully removed and safe; regression coverage: `runtime_type_matches_function_param_via_core_subtype_issue_6597` (unit), `arithmetic_narrow_int_wrapping_5205` (direct callable operators, `map(+, ...)` / `map(*, ...)`, and empty narrow-int / Bool `reduce` / `mapreduce`), `hof_reduce_fold_plus_type_preservation_4622`, `hof_mapreduce_identity_plus_type_preservation_4619`. |
| pending | #2494, #5921 | The compile-time `JuliaType::is_subtype_of` and runtime `Vm::check_subtype` no longer carry parallel hand-maintained numeric/range hierarchy tables with an "update both" sync duty: both delegate the built-in hierarchy to the shared `CoreSubtypeEngine` (single source of truth in `inference_core`). Regression gates: `test_check_subtype_parity_with_julia_type` (extended with range pairs) and the upstream-julia-verified `engine_delegation_matrix_issue_5921` matrix. |
| pending | #6272 | Interprocedural exception composition now consults the pure-Julia reflection classification (`Base._classified_exception_type`) for pure-Julia Base callees instead of Rust-side `gcd`/`lcm` name special-cases or walking their self-recursive bodies. The classification table was extended to all fixed-width integer widths so direct and user-wrapper calls match upstream. |
| pending | #6097, #6105 | Raspberry Pi 32-bit smoke now asserts native `Int`/`UInt` aliases resolve to `Int32`/`UInt32`. |
| pending | #6096 | `Sys.WORD_SIZE` is now exposed and the Raspberry Pi 32-bit smoke asserts it reports 32. |
| pending | #5484 | Predicate broadcast fixtures now assert upstream-compatible `BitVector` materialization. |
| #3323 | #3256 | Splat calls now use runtime dispatch via `PushFunction + CallFunctionVariableWithSplat` instead of first-method-only compilation |
| #3266 | #3254 | REPL globals now persist Bool/I8–I128/U8–U128/F16/F32/GlobalRef/Pairs/Set/Regex/Enum/Memory via `other_vars` catch-all |
| #3270 | #3255 | Narrow integer Return: I8/I16/I32/I128/U8–U128/Bool now emit `ReturnI64` (preserves original type) |
| #3329 | #3255 | Narrow integer locals: I8/I16/I32/I128/U8–U128 now stored in `locals_narrow_int` (not `locals_any`) |
| #3272 | #3257 | `methods(f, types)` type-filtered lookup now implemented |
| #3276 | #3260 | AoT modules now filtered by call graph — only referenced modules included in output |
| #3278 | #3261 | AoT TupleFirst/TupleLast now use dedicated AotBuiltinOp variants instead of array ops |
| #2863 | — | Enum type now maps to `ConcreteType::Enum` (was `LatticeType::Top`) |
| — | #3259 | AoT codegen now generates proper nested Vec for N-dimensional arrays (was flattened to 1D) |
| — | #3258 | AoT LICM now hoists loop-invariant instructions into CFG preheader blocks |
| — | #3761 | `repeat`, `rotl90`, and `rotr90` now allocate typed multi-dimensional results with `similar(arr, dims...)` after #3751 |
| — | #4056 | Runtime tuple `collect(x::Any)` now dispatches to `collect(::Tuple)` directly, removing the `_collect(::EltypeUnknown)` tuple guard |
| — | #4131 | `Type{Any}` singleton methods now beat bare `::Type`, so `IteratorSize` / `IteratorEltype` no longer need `T === Any` fallback branches |
| — | #4569 | Runtime dispatch now matches `Type{Array{T}}` against local `Array{Int64}` DataType values, so generic tuple-dims allocation uses `similar(Array{T}, dims)` |
| — | #4574 | Compile-time dispatch now prefers exact `Type{Any}` methods over generic `Type{T}` methods for tuple-dims helpers, so Any shaped collect allocation uses `_array_undef_from_dims(Any, dims)` |
| — | #4577 | Non-exact `Type{Any}` singleton matches are now weaker than generic `Type{T}` matches, so `_array_undef_from_dims(Symbol, dims)` preserves `Symbol` allocation |
| — | #4639 | `Vector{Int16} == Vector{Int16}` now dispatches through the legacy Array equality bridge, so the #4606 fixture uses a direct vector equality assertion |
| — | #4629 | Typed matrix literals now lower through typed vector construction plus `reshape`, so `materialization_type_preservation_4628` uses direct `Float32[1 2; 3 4]` syntax |
| — | #4675 | `dropdims` String vector equality now compares logical Array wrapper values directly, so the #4591 fixture uses direct vector equality again |
| — | #5193 | `expr::T` type assertions on call expressions now lower to `typeassert(expr, T)`, so `oftype` uses the exact upstream `convert(typeof(x), y)::typeof(x)` form again (regression: `essentials/typed_expr_typeassert.jl`) |
| pending | #7255 | Array-literal positional splat is supported in lowering (`Any[pts...]`, `[a, xs..., b]`, typed `T[...]`), so JSXGraph `polygon(pts...)` uses the idiomatic `Any[pts...]` again instead of `collect(Any, pts)` (regression: `splat/splat_array_literal_7255`). |
| pending | #7334, #7322 | A `::AbstractMatrix` method parameter no longer loose-matches a function singleton (`typeof(sin)`): the compile-time struct-parents fallback (`struct_is_subtype_of_abstract`) resolves a `typeof(...)` struct name through its known built-in `Function` supertype instead of conservatively accepting it (same class as #7266). Plots `scatter(m::AbstractMatrix)` / `scatter!(m::AbstractMatrix)` now use upstream Plots' `AbstractMatrix` again instead of the concrete `::Matrix` workaround, and `scatter(sin)` correctly reaches `scatter(f::Function)` (regression: `dispatch/dispatch_abstractmatrix_no_loose_match_function_7334`, `packages/plots_scatter_matrix_7322`). |

---

## Web Playground — iOS-only samples show unsupported notice

**File:** `web/app.js` (`showUnsupported`), `web/samples_ir.js` (`webUnsupported` flags)

```javascript
// Workaround: some iOS samples depend on packages/JS renderers not shipped in the
// static web build; show a friendly fallback instead of executing. (Issue #7286)
```

**Impact:** Samples that use `Distributions`, `Primes`, `Symbolics`, or `JSXGraph` are marked `webUnsupported: true` and display a fallback message in the browser, even though they run in upstream Julia and the `target/release/sjulia` CLI.

**Linked issue:** #7286

**Resolution path:** Include the required packages in the web/base cache, or provide web-compatible fallbacks (e.g., replace JSXGraph with Plotly, emulate package subsets in pure Julia) so the samples are no longer `webUnsupported`.

---

*Last updated: 2026-06-30. Run the audit command to find new workarounds.*
