# Active Workarounds in SubsetJuliaVM

This document catalogues all active workarounds in the VM codebase, along with their impact, location, and linked tracking issues. Workarounds are marked with `// Workaround: ...` comments in Rust source and `# Workaround: ...` comments in Julia source.

**Rust audit command:** `rg -n --glob '*.rs' "// Workaround:" subset_julia_vm/src/`

---

## Compile — Macro Helper Guarded AST Field Access

**File:** `subset_julia_vm_compile/src/compile/expr/struct_.rs`

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

**File:** `subset_julia_vm_vm/src/vm/executable.rs`

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

## Base — Irrational Symbol Extracted via Type-Name String, Not Value-Parameter Reflection

**File:** `subset_julia_vm/src/julia/base/io.jl`

```julia
# Workaround: `Irrational{sym}`'s bare symbol name (`"π"`, `"ℯ"`, ...), as
# plain text. Upstream reads this via `sym` directly (`show(io::IO,
# x::Irrational{sym}) where {sym} = print(io, sym)`,
# `julia/base/irrationals.jl`) — a Symbol-valued `where`-clause type
# variable, or equivalently `typeof(x).parameters[1]`. Both currently lose
# `Symbol` identity for *non-ASCII* symbols in sjulia (`typeof` reports
# `DataType`, and `print`/`string` render the quoted `:sym` show-form instead
# of the bare name) — exactly the case that matters here, since every
# `Irrational` singleton (`π`, `ℯ`, ...) is named with a non-ASCII symbol.
# Parse the symbol out of `string(typeof(x))` (`"Irrational{:π}"`) instead:
# that string is built from the correctly-encoded type name text, not the
# broken value-parameter reflection. (Issue #8869)
function _irrational_symbol_text(x::AbstractIrrational)
    type_name = string(typeof(x))
    # Keep both public range endpoints on character starts (Issue #11618).
    symbol_start = ncodeunits("Irrational{:") + 1
    closing_brace = prevind(type_name, ncodeunits(type_name) + 1)
    symbol_end = prevind(type_name, closing_brace)
    return type_name[symbol_start:symbol_end]
end
```

**Impact:** `show`/`string`/`print`/`repr` for `Irrational` singletons (`π`,
`ℯ`) parse the symbol out of the type's rendered name string instead of
reading the value type parameter directly. Functionally equivalent for the
two symbols that actually exist in Base (`:π`, `:ℯ`), and for any other
concrete `Irrational{sym}` a user constructs — but it is textual parsing of
`"Irrational{:sym}"` rather than reading `sym` as a first-class `Symbol`, so
it silently breaks if the `Irrational` struct or its `show`-form type-name
rendering is ever renamed/reshaped.

**Linked issue:** #8869 (Symbol-valued `where`-clause / `.parameters[1]`
type-parameter reflection loses `Symbol` identity for non-ASCII symbols)

**Resolution path:** Once #8869 is fixed (a `where {sym}`-bound or
`typeof(x).parameters[1]`-reflected Symbol-valued type parameter correctly
behaves as a `Symbol` for non-ASCII symbols too), replace
`_irrational_symbol_text` with the upstream-shaped
`show(io::IO, x::Irrational{sym}) where {sym} = print(io, sym)`.

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

## Plots — `@animate`/`@gif` Frame Counter Held in a `Ref`

`subset_julia_vm/packages/Plots/src/api.jl`, the `@animate` and `@gif` macros.

```julia
# Workaround: hold the frame counter in a `Ref` so the loop MUTATES its
# contents (`_anim_counter[] = …`, a `setindex!` — not a rebinding of the
# `_anim_counter` name) ... (Issue #9476 / #9283)
local _anim_counter = Ref(1)
```

The `@animate`/`@gif` expansion appends a per-iteration `frame(_anim, should)`
plus a counter bump to the user's top-level `for`/`while` loop. Upstream Plots
uses macro **hygiene** — a `gensym` counter treated as a local — so the counter
is never a soft-scope-ambiguous global. sjulia's macro system does not apply full
hygiene, so the counter is a literal `_anim_counter`. Once the C ABI / WASM hosts
adopted **strict file-mode soft scope** (Issue #9283 / #9210), a plain counter
assigned before the loop and `+=`-ed inside it is localized to a fresh loop-local,
and the read-before-write raises `UndefVarError: \`_anim_counter\` not defined` —
breaking the `plots_animation`, `aizawa_attractor`, and
`ordinarydiffeq_pendulum_animation` samples. The natural upstream-shaped fixes —
emit `global _anim_counter` inside the loop, or wrap the counter in a `let` — are
both rejected by sjulia's macro runtime in expansion output (Issue #9476). Holding
the counter in a `Ref` and mutating `_anim_counter[]` (a `setindex!`, which the
soft-scope pass does not treat as a name rebinding) is the upstream-valid shape the
runtime accepts, and works in every soft-scope mode and scope (top level and inside
a function).

**Resolution path:** Implement macro-runtime support for returning `Expr(:global,
…)` / `Expr(:let, …)` (Issue #9476), then restore a plain counter with a `global`
declaration (or a `let`-scoped counter) matching upstream Plots' hygienic shape.

---

## Base — `checked_mul` Promotion Entry Uses Two-Variable Form (W-58)

`subset_julia_vm/src/julia/base/checked.jl`, the mixed-type `checked_mul`
promotion entry.

```julia
# Workaround: written in the two-variable form used by the promote fallbacks in
# base/promotion.jl instead of upstream's `checked_mul(promote(x, y)...)` splat
# form — the splatted call re-dispatches to this same method instead of the
# promoted same-type diagonal method, recursing forever (Issue #9513).
function checked_mul(x::Integer, y::Integer)
    px, py = promote(x, y)
    return checked_mul(px, py)
end
```

Upstream (`julia/base/checked.jl`) writes the promotion entry as the one-liner
`checked_mul(x::Integer, y::Integer) = checked_mul(promote(x,y)...)`. In sjulia
the splatted self-recursive call re-selects the same `(Integer, Integer)` method
for the promoted same-type pair instead of the more-specific diagonal
`checked_mul(x::T, y::T) where {T<:Integer}` method, so any mixed-type call
(e.g. `checked_mul(4, UInt128(5))`) recursed to a StackOverflow. The
two-variable form (`px, py = promote(x, y); checked_mul(px, py)`) dispatches
correctly — the same form all promote fallbacks in `base/promotion.jl` use.

**Resolution path:** Fix splatted-call runtime dispatch to use the tuple's
runtime element types (Issue #9513), then restore upstream's splat one-liner.

---

## Tests — Array Rank Dispatch Forced Through `Any` (W-59)

**File:** `tests/fixtures/dispatch/dispatch_agg_misc_10238.jl` (former standalone
`dispatch/array_dimension_dispatch.jl`, module-wrapped into the aggregate by
Issue #10238)

```julia
# Workaround: force runtime dispatch for the 3D Array case because static call
# resolution drops Array rank for similar-created arrays (Issue #9642)
rank_dispatch_any(x::Any) = rank_dispatch(x)
```

**Impact:** The fixture routes only the 3D `similar(v, 1, 3, 1)` dispatch check
through an `Any`-typed wrapper so sjulia uses runtime dispatch and matches
upstream Julia's `Array{Int64,3}` method selection. Direct static dispatch still
drops the array rank and selects the 1D method; that compiler bug is tracked in
#9642.

**Resolution path:** Fix static call resolution to preserve or defer dispatch for
`Array{T,N}` rank on `similar(a, dims...)` results (Issue #9642), then call
`rank_dispatch(a3)` directly and remove this wrapper.

---

## Base — UInt128 String Digit Loop Avoids `rem` (W-60)

**File:** `subset_julia_vm/src/julia/base/intfuncs.jl`

```julia
# Workaround: compute the UInt128 remainder via div/mul/sub. Direct
# rem(UInt128, UInt128) still errors for large dividends, while mixed
# UInt128/Int rem can enter user-exported promote_rule fallbacks and
# recurse. (Issue #9770; Issue #9333)
q = div(n, b)
d = n - q * b
```

**Impact:** `string(n::Integer; base=..., pad=...)` stays in Pure Julia and
handles full-width `UInt128` values by using the already validated small base
as a `UInt128` divisor for `div`, then deriving the digit remainder with
`n - q * b`. This matches the observable digit conversion behavior, but the
implementation avoids both direct `rem(::UInt128, ::UInt128)` for large
dividends and mixed `UInt128`/`Int` `rem` dispatch after packages export their
own `promote_rule`.

**Linked issues:** #9770, #9333

**Resolution path:** Once #9770 fixes `rem(::UInt128, ::UInt128)` for large
dividends and #9333 fixes promotion fallback isolation, replace the div/mul/sub
digit remainder with the upstream-shaped integer-to-string helper.

---

## Tests — Generator Trait Matrix Cells Evaluated Through Helpers (W-61)

**File:** `subset_julia_vm/tests/fixtures/generator/generator_trait_matrix_9566.jl`

```julia
# Workaround: evaluate generated generator cells through function-scope
# helpers because @testset block scope loses lifted generator body
# bindings for trait queries (Issue #10137).
```

**Impact:** The generated #9566 matrix keeps `@testset` grouping, but each cell
expression is evaluated through a generated helper function instead of directly
inside the `@testset` block. This preserves the same upstream expression result
while avoiding the current block-scope lifted-body lookup gap for generator trait
queries. The workaround is confined to this test fixture generator and does not
change VM behavior.

**Linked issue:** #10137

**Resolution path:** Once #10137 fixes generator trait queries inside `@testset`
block scope, regenerate the fixture without per-cell helper functions and remove
this entry.

---

## VM — Complex{Float64} `*` / `abs2` Fast Path on Abstract `::Complex` Route

**File:** `subset_julia_vm_vm/src/vm/exec/call.rs`, `subset_julia_vm_vm/src/vm/exec/binary_both.rs`

*Resolved by #10530 — the `abs2` side is retired; the `*` side still falls
back to `CallDynamicBinaryBoth` and normal frame dispatch until the generic
`Base.*`/`Base.+` bodies can be predecoded. See the Resolved Workarounds
table below.*

**Linked issue:** #10530

**Resolution path:** `try_complex_f64_resolved_call_fast_path` and
`try_complex_f64_abs2` were removed. `Base.abs2(::Complex{Float64})` runs
through the normal `execute_direct_call_fast` →
`try_execute_typed_scalar_function_call` path because the concrete method
has a `Struct` parameter and is not a runtime-specialization candidate.
Generic `Base.*/Base.+` remain out of scope for the typed-scalar path; a
follow-up Issue tracks making their bodies predecodable.

---

## Base — Rational Raw Allocation Uses a Marker-Token Inner (W-70)

**File:** `subset_julia_vm/src/julia/base/rational.jl`

```julia
# Workaround: keep raw allocation in a same-name marker-token inner until differently named struct-body helpers can use `new` (Issue #11005).
```

**Impact:** `Rational{T}` keeps its public two-argument constructor topology
aligned with upstream dispatch by reserving raw allocation for a private
three-argument inner constructor whose first argument is an unexported marker.
`unsafe_rational` supplies that marker after normalization. The extra marker is
an internal implementation detail and does not alter the public constructor API.

**Linked issue:** #11005

**Resolution path:** Once a differently named helper declared in a struct body
can use `new`, move raw allocation into the upstream-shaped `unsafe_rational`
helper and remove the marker type, marker argument, and this entry.

---

## Compiler — Runtime Explicit-Constructor Dispatch Lacks Candidate Bindings (W-71)

**File:** `subset_julia_vm_compile/src/compile/expr/call/constructors.rs`

```rust
// Workaround: runtime-Any explicit constructor overloads still throw. (Issue #10971)
```

**Impact:** Explicit parametric constructor overloads dispatch correctly when
value argument types are statically known. If the value argument is inferred as
`Any`, sjulia raises a catchable `MethodError` instead of deferring across the
overload candidates, because the existing runtime typed-dispatch instruction
cannot attach the selected method's explicit constructor-self bindings.

**Linked issue:** #10971

**Resolution path:** Add a runtime dispatch operand/instruction that carries
candidate-specific `StaticParamBinding` lists, bind the selected constructor
self variables after value-signature selection, and add the Issue MWE as a
fixture before removing this branch/comment.

---

## Compiler — Base Constructor Rows Keep Legacy Callable-Self Identity (W-72)

**Files:** `subset_julia_vm_compile/src/compile/pipeline_ctx.rs`,
`subset_julia_vm_compile/src/compile/expr/call/constructors.rs`

```rust
// Workaround: keep Base constructor rows on legacy identity. (Issue #11062)
```

**Impact:** User-defined explicit and synthesized default inner constructors
retain their complete callable-self pattern for alias, binder-bound, and
module-owner identity. Base constructors continue using the projected identity
path, and synthesized defaults remain user-only; applying either new route to
Base currently makes value-parameterized carriers such as `SubArray` and
`HasShape{N}` lose parameters, makes `UnitRange{T}` recursively select itself,
and causes broad Array/broadcast/Channel stack overflows.

**Linked issue:** #11062

**Resolution path:** Make complete constructor-self identity Base-cache and
runtime-routing safe, then prove the comprehension MWE plus the full fixture
and cache-sensitive lanes before removing both comments and this entry.

---

## Bundled StaticArrays — SMatrix Constructor Chain Avoids Splat-Forwarding Into A Value-Parameter Curly (W-74)

**Files:** `subset_julia_vm/packages/StaticArrays/src/SMatrix.jl`

```julia
# Workaround: pass `xs`/`xs[1]` as a single Tuple argument rather than
# re-splatting `xs...` forward — splatting a vararg collection into a
# runtime type-application curly whose trailing slot is a value
# expression fails to resolve the expression (Issue #11539). (Issue #11539)
```

**Impact:** `SMatrix{M,N,T}`'s outer constructor forwards to the
fully-parameterized `SMatrix{M,N,T,L}` constructor by passing the collected
`xs`/`xs[1]` vararg tuple as a single positional argument, rather than the
leaner `SMatrix{M,N,T,length(xs)}(xs...)` re-splat form. Both forward the
same values and produce identical results; the non-splat form costs one
extra internal single-tuple unwrap/re-dispatch hop per `SMatrix` construction
call, slightly deepening the sjulia call stack for the many `SMatrix` values
built through deep interpreter call chains (e.g. Interact/Plots pipelines).

**Linked issue:** #11539 (discovered while implementing #11432)

**Resolution path:** Fix runtime type-application resolution so a splatted
vararg forward (`Type{...,expr}(xs...)`) resolves a trailing value-parameter
expression the same way the non-splat form (`Type{...,expr}(xs)`) already
does, then switch `SMatrix{M,N,T}`/`SMatrix{M,N}` to splat-forward and remove
this entry and comment.

---

## Base — ParseError Keeps An Explicit Field Constructor Under A Nested Same-Leaf Type (W-75)

**File:** `subset_julia_vm/src/julia/base/error.jl`

```julia
# Workaround: keep the Base-owned two-field constructor explicit so a same-leaf JuliaSyntax.ParseError does not hide the cached default. (Issue #10445)
ParseError(msg::AbstractString, detail) = new(msg, detail)
```

**Impact:** `Base.ParseError` spells out the ordinary two-field inner
constructor that upstream receives implicitly. Runtime behavior and dispatch
remain the same, but the explicit row prevents the newly restored nested
`JuliaSyntax.ParseError` detail type from hiding Base's cached default
constructor in sjulia's still-partially bare-name constructor tables.

**Linked issue:** #10445 (reconfirmed while implementing #11572)

**Resolution path:** Once constructor-family lookup is owner-scoped for cached
synthetic defaults as well as explicit inner allocation targets, remove the
explicit constructor and prove both the #10445 same-leaf MWE and the
`test_exception_types.jl` / `exceptions_parseerror_detail_11572.jl` fixtures.

---

## Tests — Evaluate `@isdefined` Before `@assert` (W-77)

**File:** `subset_julia_vm/tests/fixtures/types/runtime_nominal_control_flow_11654.jl`

```julia
# Workaround: evaluate @isdefined outside @assert until nested macro expansion is supported (Issue #11677)
for_a_defined11654 = @isdefined for_a11654
@assert !for_a_defined11654
```

**Impact:** The #11654 parity fixture stores each `@isdefined` result before
asserting it. This preserves the same boolean check as upstream Julia but does
not directly exercise an `@isdefined` macro nested inside `@assert`, which
sjulia currently rejects during lowering.

**Linked issue:** #11677

**Resolution path:** Once #11677 supports nested macro expansion inside
`@assert`, inline both `@isdefined` calls into their assertions and remove this
entry.

---

## Bundled StaticArraysCore — SMatrix Constructor Chain Avoids Splat-Forwarding Into A Value-Parameter Curly (W-76)

**Files:** `subset_julia_vm/packages/StaticArraysCore/src/types.jl`

```julia
# Workaround: pass `xs`/`xs[1]` as a single Tuple argument rather than
# re-splatting `xs...` forward — splatting a vararg collection into a
# runtime type-application curly whose trailing slot is a value
# expression fails to resolve the expression. (Issue #11539)
```

**Impact:** `SMatrix{M,N,T}`'s outer constructor forwards to the
fully-parameterized `SMatrix{M,N,T,L}` constructor by passing the collected
`xs`/`xs[1]` vararg tuple as a single positional argument, rather than the
leaner `SMatrix{M,N,T,length(xs)}(xs...)` re-splat form (the same shape
Issue #11539 documents for bundled StaticArrays' independent `SMatrix`, PR
#11543). Both forward the same values and produce identical results; the
non-splat form costs one extra internal single-tuple unwrap/re-dispatch hop
per `SMatrix` construction call.

**Linked issue:** #11539 (discovered while implementing #11432; re-applied
here for the separate, independent StaticArraysCore package while
implementing #11542)

**Resolution path:** Fix runtime type-application resolution so a splatted
vararg forward (`Type{...,expr}(xs...)`) resolves a trailing value-parameter
expression the same way the non-splat form (`Type{...,expr}(xs)`) already
does, then switch `SMatrix{M,N,T}`/`SMatrix{M,N}` to splat-forward and remove
this entry and comment.

---

## Base — SubstitutionString Omits 1-Arg `codeunit` And Adds A 1-Arg `hash` (W-78, W-79)

**File:** `subset_julia_vm/src/julia/base/strings/util.jl`

```julia
# Workaround: upstream also defines 1-arg `codeunit(s::SubstitutionString) =
# codeunit(s.string)` (the code-unit *type* query), but sjulia's codeunit
# builtin rejects the 1-arg form at compile time, so it is omitted here
# (Issue #11751).
...
# Workaround: the 1-arg method is needed because sjulia's `hash(x)` is the
# `_hash` builtin and does not forward through 2-arg dispatch like upstream's
# `hash(x) = hash(x, zero(UInt))` (Issue #11754).
Base.hash(s::SubstitutionString) = hash(s.string)
```

**Impact:** The SubstitutionString AbstractString surface (Issue #10735)
diverges from the upstream base/regex.jl method set in two spots: the 1-arg
code-unit-type query is missing (W-78), and an extra 1-arg `hash` method exists
that upstream gets generically (W-79). Behavior for all fixture-covered
operations matches upstream.

**Linked issues:** #11751 (1-arg `codeunit` unsupported), #11754 (1-arg `hash`
does not forward through 2-arg dispatch)

**Resolution path:** When #11751 lands, add the upstream 1-arg `codeunit`
method; when #11754 lands, delete the 1-arg `hash` method. Re-run the
`regex_substitution_string_abstractstring_10735` fixture both times.

---

## Tests — SubstitutionString Fixture Hoists Literals Out Of Macro Arguments (W-80)

**Files:** `subset_julia_vm/tests/fixtures/regex/substitution_string_abstractstring_10735.jl`,
`subset_julia_vm/tests/fixtures/regex/match_findnext_index_errors_10736.jl`

```julia
# Workaround: regex and s"..." literals are hoisted to variables because inside
# a macro call argument an r"..." fails lowering (Issue #11753) and an s"..."
# silently degrades to a plain String (Issue #11756).
...
# Workaround: the expected repr value is hoisted because an escaped-quote
# string literal inside a macro call argument is mangled (Issue #11757).
```

**Impact:** The #10735 fixture assigns `r"..."` / `s"..."` / escaped-quote
string literals to variables before using them in `@assert` expressions. The
asserted values are identical to the inline upstream form; only the macro-arg
literal-lowering family of gaps is routed around.

**Linked issues:** #11753 (`r"..."` in macro args fails lowering), #11756
(`s"..."` in macro args degrades to String), #11757 (escaped quotes in macro
args mangled)

**Resolution path:** As each issue lands, inline the corresponding literal back
into the assertions and remove the matching comment; remove this entry when all
three are inlined.

---

## VM — SubString Conversion Uses The String Carrier (W-81)

**File:** `subset_julia_vm_vm/src/vm/convert.rs`

```rust
// Workaround: preserve the String carrier until SubString has a runtime value (Issue #11783).
"SubString{String}" => convert_to_string(value),
// Workaround: accept the String carrier as a converted SubString Union member (Issue #11783).
```

**Impact:** `convert(SubString{String}, s::String)` can participate in typed
literal and Union element conversion, preserving the value and the array's
logical `SubString{String}` element tag. The returned scalar still uses
sjulia's String carrier, so direct `typeof(convert(SubString{String}, s))`
cannot yet expose a distinct upstream SubString value.

**Linked issue:** #11783

**Resolution path:** Add a first-class SubString runtime value plus upstream's
`base/strings/substring.jl` constructors/conversion methods, then delete this
VM conversion arm and its registry entry.

---

## Summary Table

| ID | Category | File | Impact | Linked Issue |
|----|----------|------|--------|--------------|
| W-81 | VM | `subset_julia_vm_vm/src/vm/convert.rs` | SubString conversion preserves the String carrier until sjulia models distinct SubString runtime values | #11783 |
| W-80 | Tests | `tests/fixtures/regex/substitution_string_abstractstring_10735.jl`, `tests/fixtures/regex/match_findnext_index_errors_10736.jl` | #10735/#10736 fixtures hoist r"..."/s"..."/escaped-quote literals out of macro arguments until the macro-arg literal-lowering gaps land | #11753, #11756, #11757 |
| W-79 | Base | `subset_julia_vm/src/julia/base/strings/util.jl` | SubstitutionString defines a 1-arg `hash` because sjulia's `hash(x)` builtin does not forward through 2-arg dispatch | #11754 |
| W-78 | Base | `subset_julia_vm/src/julia/base/strings/util.jl` | SubstitutionString omits upstream's 1-arg code-unit-type `codeunit` method until the builtin accepts the 1-arg form | #11751 |
| W-77 | Tests | `tests/fixtures/types/runtime_nominal_control_flow_11654.jl` | Runtime nominal fixture stores `@isdefined` results before `@assert` until nested macro expansion works | #11677 |
| W-75 | Base | `subset_julia_vm/src/julia/base/error.jl` | Base.ParseError keeps an explicit two-field constructor so nested JuliaSyntax.ParseError cannot hide its cached default row | #10445 |
| W-74 | Base | `subset_julia_vm/packages/StaticArrays/src/SMatrix.jl` | SMatrix constructor chain passes a single Tuple argument instead of splat-forwarding into a value-parameter curly slot | #11539 |
| W-70 | Base | `subset_julia_vm/src/julia/base/rational.jl` | Raw Rational allocation uses a private marker-token inner until differently named struct-body helpers can use `new` | #11005 |
| W-71 | Compiler | `compile/expr/call/constructors.rs` | Runtime-`Any` explicit constructor overloads raise `MethodError` until typed dispatch can carry candidate-specific self bindings | #10971 |
| W-72 | Compiler | `compile/pipeline_ctx.rs`, `compile/expr/call/constructors.rs` | Complete callable-self metadata and synthesized default rows remain user-only until Base value parameters and UnitRange/Array routes are cache/runtime safe | #11062 |
| W-76 | Base | `subset_julia_vm/packages/StaticArraysCore/src/types.jl` | StaticArraysCore SMatrix constructor chain passes a single Tuple argument instead of splat-forwarding into a value-parameter curly slot | #11539 |
| W-66 | VM | `subset_julia_vm_vm/src/vm/exec/call.rs`, `subset_julia_vm_vm/src/vm/exec/binary_both.rs` | Abstract `::Complex` Mandelbrot loops use a Rust fast path for resolved `*` / `abs2` calls until pure-Julia Complex dispatch is fast enough (#10530) | #10530 |
| W-61 | Tests | `tests/fixtures/generator/generator_trait_matrix_9566.jl` | Generated #9566 matrix evaluates cells through helper functions until `@testset` block-scope generator trait queries keep lifted body bindings | #10137 |
| W-60 | Base | `julia/base/intfuncs.jl` | `string(n; base=...)` computes the UInt128 digit remainder via `div`/`mul`/`sub` until `rem(::UInt128, ::UInt128)` works for large values and mixed UInt128/Int `rem` no longer falls into user `promote_rule` recursion | #9770, #9333 |
| W-59 | Tests | `tests/fixtures/dispatch/dispatch_agg_misc_10238.jl` | 3D Array rank dispatch fixture forces runtime dispatch through `Any` until static call resolution preserves `Array{T,N}` rank for `similar` results (former `array_dimension_dispatch.jl`, module-wrapped into the aggregate by #10238) | #9642 |
| W-58 | Base | `julia/base/checked.jl` | Mixed-type `checked_mul` promotion entry uses the two-variable `px, py = promote(x, y)` form instead of upstream's `checked_mul(promote(x,y)...)` splat one-liner, because the splatted self-recursive call re-dispatches to the same method (StackOverflow) | #9513 |
| W-55 | Plots | `packages/Plots/src/api.jl` | `@animate`/`@gif` hold the frame counter in a `Ref` and mutate `_anim_counter[]` (a `setindex!`) instead of a plain `_anim_counter += 1`, because under strict file-mode soft scope a plain top-level counter is localized to a fresh loop-local (`UndefVarError`), and the macro runtime rejects `global`/`let` in expansion output | #9476, #9283 |
| W-53 | Base | `julia/base/io.jl` | `Irrational` singleton (`π`, `ℯ`) `show`/`string` parses the symbol out of `string(typeof(x))` text instead of reading it as a first-class `Symbol` value parameter | #8869 |
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
| pending | #9342/#11481 | Complex `sqrt(conj(...))` W-54 removed: the builtin `sqrt` compiler path now emits filtered runtime typed dispatch plus the builtin fallback whenever the operand is not statically proven to be a primitive real numeric value. `asin`/`acos`/`acosh` use the upstream-shaped `sqrt(conj(w))` expressions again, and exact runtime `Complex{T}` values are no longer dependent on a hard-coded constructor inference result. Regression: `complex_inverse_trig_8813`, `constructor_return_exact_or_any_11436`. |
| pending | #11432 | IFS sample unparameterized-`SMatrix` field annotation (W-73) removed: bundled `StaticArrays.SMatrix` now declares the upstream fourth length parameter (`struct SMatrix{M,N,T,L} <: StaticMatrix{M,N,T}`, `L == M*N`), matching the canonical `StaticArraysCore` alias shape. `SMatrix{M,N,T}` (and narrower spellings) stay constructible via incomplete parameterization — sjulia's existing partial-`UnionAll`-application dispatch already generalizes to the added trailing parameter, so no other `StaticArrays` dispatch site needed a signature change. Two Rust-side fast paths that recognize `SMatrix` by parsing its display-name string had to be updated for the new `"SMatrix{M, N, T, L}"` shape: `try_make_static_array` (`subset_julia_vm_vm/src/vm/exec/struct_ops.rs`, the small-array inline-representation intercept) and the `StaticArrayInlineData`/`StaticRealValue` type-name tables and `elem_type_str` element-type extractor (`subset_julia_vm_bytecode/src/value/static_real.rs`) — both previously assumed exactly three curly parameters. `W::SMatrix{2,2,Float64,4}` restored in all three synchronized IFS sample copies. Regression: `static_arrays_smatrix_four_param_11432`. |
| #11012 | #10969 | Caller type-binding forwarding W-67 removed: constructor-self family is serialized with each `MethodTable`, and dynamic constructor selection searches the sibling explicit-parametric outer table for both user and cached Base methods. Cached `Rational{T}` normalizes `2//4` to `1//2`; struct-backed UnitRange/StepRange indices are handled through the `AbstractRange` protocol (#10970). |
| pending | #10298 | WeakKeyDict array-key assertion workaround (W-63) removed: expression inference now treats an array-valued index as proving an array result only when the receiver is known to be array-like. Custom `getindex` receivers remain dynamic, so `wkd[array_key] == "ARRAY-KEY"` executes runtime equality instead of folding to `false`. Regression: `ref_weakref_finalizer_weakkeydict_8990`. |
| pending | #10261 | W-65 retired: dependent bounds now reference the same owner-scoped runtime TypeVar objects (`B.ub === A`, `C.ub === B`), and `_typejoin_subst_dependent_bound` substitutes by direct `===` identity. The whole-result subtype guard remains as a general fail-closed check. Regression: `types_typejoin_dependent_bound_name_collision_10252`. |
| pending | #10530 | Complex{Float64} `*` / `abs2` resolved-call fast path (W-66) retired: `try_complex_f64_resolved_call_fast_path` and `try_complex_f64_abs2` removed. `Base.abs2(::Complex{Float64})` runs through the normal `execute_direct_call_fast` → `try_execute_typed_scalar_function_call` path; the concrete method has a `Struct` parameter and is not a runtime-specialization candidate, so no guard bypass was needed. Generic `Base.*/Base.+` still fall back to `CallDynamicBinaryBoth` and normal frame dispatch until their bodies can be predecoded (follow-up Issue). Regression: `mandelbrot_tests`, `benchmarks/mandelbrot_bench_untyped.jl`. |
| pending | #10631 | Partition-destructuring fixture workaround (W-69) removed after tuple equality began recursively comparing array-valued elements across concrete element-type differences. Regression: `flat_nonliteral_destructure_ir_10464` now directly compares the yielded chunk tuple with `([1, 2], [3, 4])`. |
| pending | #10558 | Zero-argument concrete-type `Core.apply_type` workaround (W-68) removed: `Core.TypeName.wrapper` now projects the generic struct or abstract-type `UnionAll` needed by `typejoin`, and `Core.apply_type(concrete)` consistently raises `TypeError`. Canonical `wrapper === original_wrapper` identity remains open under #10558. Regression: `types_apply_type_dynamic_splat_10191`. |
| pending | #10191 | Dynamic-base `Core.apply_type` splat workaround (W-64) removed: `ApplyTypeDynamicSplat` expands the complete call-argument splat mask, selects the base after flattening (#10555), and applies the remaining flat parameter list to the runtime `UnionAll` with bound/arity/concrete-base validation (#10554), so `typejoin` now uses the upstream-shaped `Core.apply_type(wrapper, subst...)` directly. Regression: `types_apply_type_dynamic_splat_10191`, including negative parity cases and a 17-parameter wrapper. |
| pending | #10242 | Test macro `__test_*`-prefixed quote-internal names (W-67) removed: `collect_introduced_vars` (`subset_julia_vm_lowering/src/lowering/expr/quote/handlers.rs`) now has an `ExprHead::Try` arm that registers the `catch` variable as a hygiene-renamed local (gensym'd) the same way `local`/assignment targets already were, closing the gap where the static stdlib-macro quote expansion left `catch e` unrenamed. `Test.@test`/`@test_throws`/`@test_broken` in `julia/stdlib/Test/src/Test.jl` reverted to natural names (`result`, `threw`, `e`, ...); the renamed catch variable can no longer collide with a same-named user/global (e.g. `Base.MathConstants.e`). Regression: `tests/testset_exit_code_8191_tests.rs::test_macro_catch_variable_does_not_shadow_user_e_10242`, `tests/fixtures/stdlib/test_errored_expr_10093.jl`, and the new `tests/fixtures/stdlib/test_catch_hygiene_10242.jl`. |
| pending | #10208 | `Test.@test` nested if/else workaround (W-66) removed: the static stdlib-macro quote expansion (`lowering/expr/quote/code_generation.rs`, `handlers.rs`) and the nested-macro-calling-macro path (`lowering/expr/macros/nested.rs`) both gained `Expr(:elseif, ...)` handling — the wrapped `Expr(:block, condition)` unwraps through the existing single-statement block handling, and the clause lowers identically to a plain `if` nested in the parent's else-branch position, matching upstream's `elseif` desugaring. `Test.@test`'s errored-outcome expansion restored the natural `if/elseif/else` form. Regression: `lowering::expr::macros::nested::tests::test_qctc_if_*` / `test_qctc_elseif_*` (unit), `testset_exit_code_8191_tests.rs` (`errored_test_does_not_propagate_and_flags_failure_10093`, `nonbool_test_records_errored_outcome_10093`) exercise the restored `elseif` chain end-to-end. |
| pending | #10164 | Batched prelude doc-registration filter (W-62) removed: `Lowering::lower_source_file_inner` now captures a top-level docstring into `pending_doc` the same way `LoweringWithInclude::lower_source_file_inner` already did, so both lowering paths agree and `merge_program_fragment_into` no longer needs to filter `__sjulia_doc_*` statements back out of the batched per-file prelude. A shared second bug was fixed alongside it: neither lowering path's `ConstStatement` arm called `push_doc_registration`, so a docstring preceding a top-level `const` (e.g. Base's `VERSION`) leaked past the const into whatever later definition consumed `pending_doc` next instead of documenting the const itself. A blanket `if is_docstring_target_kind(kind) { pending_doc = None; }` safety net closes the same bug class for the remaining docstring-target kinds whose arms still don't call `push_doc_registration` (non-`const` type-alias `Assignment`, the plain-`Assignment`/`MacroCall` catch-all, `@kwdef`) — no occurrence currently exists in Base source, but a leftover docstring there is now dropped instead of risking misattribution. Base's own docstrings (`Val`, `Exception`, `BoundsError`, `VERSION`, etc.) are now registered and retrievable via `@doc`. Regression: `lowering::tests::lower_source_file_captures_top_level_docstring_10164` (unit), `lowering::tests::dangling_docstring_before_plain_assignment_does_not_leak_to_next_definition_10164` (safety net, red/green verified), `pipeline::tests::prelude_batched_lowering_matches_whole_text_lowering_10119` (parity, now with real doc-registration content instead of both sides filtered), `macros_base_docstring_registration_10164` (fixture). |
| pending | #9782 | String keyword-default workaround (W-61) removed: HOF/callback dispatch and generator-fused composed calls now bind keyword defaults through the regular `bind_kwargs_defaults` path before entering methods such as `string(n::Integer; base=10, pad=1)`. Regression: `hof/builtin_as_hof_argument.jl`, `hof/hof_arity_resolution.jl`, and `generator/map_over_eager_generator_5138.jl` cover `map(string, ...)`. |
| pending | #9533 | Varargs `hypot` W-56 restored to upstream's `float.(promote(x, y, xs...))` spelling after tuple-only broadcast began materializing `Tuple` results instead of `Vector`/`BitVector` or unary tuple-literal MethodError. Regression: `broadcast/tuple_materialize_shape_9533.jl`. |
| pending | #9460/#9564 | VersionNumber show-form workaround (W-57) removed: the VM records `Base.print(io::IO, ::T)` methods separately from `Base.show(io::IO, ::T)`, print paths prefer print methods and fall back to show, and display registry keys now feed candidate lists into runtime method specificity instead of overwriting duplicate keys. Regression: `version/prerelease_show_9371_9372.jl`, `io/print_show_dispatch_split_9460.jl`, `io/display_registry_method_dispatch_9564.jl`. |
| pending | #8848 | `Vector{Any}` erased-element dispatch (W-52) restored to full array invariance: the two explicit `if matches!(pattern_elem, …Any) { return true; }` exception blocks in `comparison.rs` and `core_match.rs` were removed, and the six sjulia Base methods in `set.jl` (`union`, `intersect`, `setdiff`, `symdiff`, `unique`, `unique(f,…)`) widened from `Vector{Any}` to `AbstractVector`; the three redundant `Vector{Any}` rotation overloads in `array.jl` removed (already covered by `Array` / generic fallbacks). `Vector{String} <: Vector{Any}` is now correctly false in dispatch (Issue #8848 fixed). Regression: `dispatch_vector_any_erased_element_invariance_8848`. |
| pending | #8539 | HCubature dimension-via-`length(a)` (W-51) restored to the direct `where`-clause value parameter: `cubrule(N, T)`, `if N == 0`, `fill(1, N)`, and the `1:N` odometer loop now use `N` from the `SVector{N,T}` signature. Two compile positions were fixed: (a) a call argument whose static type is `DataType` but whose expression is a bare type-parameter variable no longer compiles to a guaranteed `ThrowMethodError` on a static dispatch miss — it routes to runtime typed dispatch, where `bind_type_params` has already bound the integer value (Issue #6625); (b) `compile_expr_as(…, I64)` accepts `DataType -> I64` via `DynamicToI64` (pass-through for the runtime `I64`, runtime error for a genuine type object — matching Julia's runtime MethodError timing for e.g. `1:Float64` range endpoints and `Vector{T}(undef, N)` dims). Regression: `types_value_type_param_positions_8539`, `packages_hcubature_ndim_8524`, `packages_hcubature_smoke_8506`. |
| pending | #8516 | DataStructures `BinaryMaxHeap` W-50 helper removed: qualified explicit parametric constructors now fall back from `Module.Type{T}` to the short constructor method table when the module-defined parametric struct has inner constructors, dotted type arguments stay static, and `typeof(local)` folds to a static type argument when the local has a known concrete type. `DataStructures.BinaryMaxHeap{T}()` and HCubature's upstream-style `DataStructures.BinaryMaxHeap{typeof(firstbox)}()` now dispatch to the Julia constructor method directly. Regression: `packages_data_structures_binary_max_heap_8509`, `packages_hcubature_smoke_8506`. |
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
