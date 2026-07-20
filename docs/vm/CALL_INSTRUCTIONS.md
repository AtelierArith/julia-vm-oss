# Call Instructions Guide

This document describes the call instruction handlers in the VM and the requirements for each.

## Call Instruction Inventory

| Instruction | File | Purpose |
|-------------|------|---------|
| `Call` | `exec/call.rs` | Basic function call (no kwargs at call site) |
| `CallWithKwargs` | `exec/call.rs` | Call with keyword arguments |
| `CallWithKwargsSplat` | `exec/call.rs` | Call with kwargs and splat expansion |
| `CallWithSplat` | `exec/call.rs` | Call with positional splat (no kwargs at call site) |
| `CallSpecialize` | `exec/call.rs` | Lazy AoT specialization |
| `CallIntrinsic` | `exec/call.rs` | Core intrinsic function call |
| `CallBuiltin` | `builtins_exec.rs` | Builtin function call |
| `CallDynamic` | `call_dynamic.rs` | Dynamic dispatch (single arg) |
| `CallDynamicOrBuiltin` | `call_dynamic.rs` | Unary dispatch with builtin fallback (length/size/ndims/eltype/similar, NegAny, floor/ceil/round/trunc) |
| `CallTypedDispatchOrBuiltin` | `call_dynamic_typed.rs` | Multi-arg typed dispatch with builtin fallback |
| `IterateDynamic` | `call_dynamic.rs` | Dynamic iterate() dispatch (1 or 2 args) |
| `CallDynamicBinary` | `call_dynamic_binary.rs` | Dynamic dispatch (binary ops, one Any operand) |
| `CallDynamicBinaryBoth` | `binary_both.rs` | Dynamic dispatch (both Any operands) + intrinsic fallback |
| `CallDynamicBinaryNoFallback` | `binary_no_fallback.rs` | Dynamic dispatch (no builtin fallback) |
| `CallTypedDispatch` | `call_dynamic_typed.rs` | Type{T} parametric dispatch |
| `CallTypeConstructor` | `call_dynamic_typed.rs` | T(x) type constructor call |
| `CallGlobalRef` | `call_function_variable.rs` | GlobalRef-based function call |
| `CallFunctionVariable` | `call_function_variable.rs` | Call function stored in variable |
| `CallFunctionVariableWithSplat` | `call_function_variable.rs` | Function variable call with splat args |

`call_dynamic_binary.rs` is the binary-dispatch entry point; it delegates
`CallDynamicBinaryBoth` to `binary_both.rs` and
`CallDynamicBinaryNoFallback` to `binary_no_fallback.rs`.

## Handler Requirements Checklist

When implementing or modifying a call instruction handler, ensure the following:

### 1. Positional Arguments

- [ ] Pop arguments from stack in correct order
- [ ] Bind arguments to parameter slots via `bind_value_to_slot()`
- [ ] Handle varargs: collect remaining args into `Value::Tuple`

### 2. Keyword Arguments

**CRITICAL**: Use the shared helper functions, not inline logic!

| Call Site | Helper Function | Used By |
|-----------|-----------------|---------|
| No kwargs provided | `bind_kwargs_defaults()` | `Call`, `CallWithSplat` |
| Kwargs provided | `bind_kwargs_with_map()` | `CallWithKwargs`, `CallWithKwargsSplat` |

These helpers handle:
- Required kwargs → `UndefKeywordError`
- kwargs varargs → empty `Pairs` or collected remaining kwargs
- Regular kwargs → provided value or default

### 3. Type Parameters

For functions with `where T` clauses:
- [ ] Extract type parameter bindings from arguments
- [ ] Handle `Val{N}` pattern for value-level type params
- [ ] Store bindings in frame for use in function body

### 4. Frame Setup

- [ ] Create frame with correct `local_slot_count`
- [ ] Push return IP to `return_ips` stack
- [ ] Push frame to `frames` stack
- [ ] Set `ip` to function entry point

## Shared Helper Functions

### `bind_kwargs_defaults()`

```rust
fn bind_kwargs_defaults(
    func: &FunctionInfo,
    frame: &mut Frame,
    struct_heap: &mut Vec<StructInstance>,
) -> Result<(), VmError>
```

**When to use**: When no kwargs are provided at the call site.

**Behavior**:
1. For each kwparam in function's kwparams:
   - If required → return `UndefKeywordError`
   - If `is_varargs` → bind empty `Pairs` (NOT `Nothing`!)
   - Otherwise → bind `kwparam.default`

### `bind_kwargs_with_map()`

```rust
fn bind_kwargs_with_map(
    func: &FunctionInfo,
    kwargs_map: &HashMap<String, Value>,
    frame: &mut Frame,
    struct_heap: &mut Vec<StructInstance>,
) -> Result<(), VmError>
```

**When to use**: When kwargs are provided at the call site.

**Behavior**:
1. For each kwparam in function's kwparams:
   - If `is_varargs` → collect remaining kwargs not matched to named kwparams into `Pairs`
   - If key in map → bind provided value
   - If required and not in map → return `UndefKeywordError`
   - Otherwise → bind `kwparam.default`

## Testing Checklist

When modifying call instruction handlers, add tests covering:

- [ ] Positional args only
- [ ] Kwargs only (named kwargs)
- [ ] Mixed positional + kwargs
- [ ] kwargs varargs with 0 kwargs passed
- [ ] kwargs varargs with multiple kwargs passed
- [ ] Positional splat (`f(args...)`)
- [ ] Positional splat with kwargs function (`f(x; kwargs...)` called via `f(args...)`)
- [ ] Kwargs splat (`f(; dict...)`)
- [ ] Required kwargs (error case)

### Fixture Test Locations

| Test | File |
|------|------|
| kwargs varargs empty | `tests/fixtures/kwargs/kwargs_varargs_empty.jl` |
| kwargs varargs with splat | `tests/fixtures/kwargs/kwargs_varargs_with_splat.jl` |
| kwargs multiple explicit | `tests/fixtures/kwargs/multiple_explicit.jl` |
| kwargs shorthand | `tests/fixtures/kwargs/shorthand.jl` |

## Code Review Checklist

When reviewing PRs that modify call instruction handlers:

- [ ] Are shared helper functions used instead of inline kwargs logic?
- [ ] Is `bind_kwargs_defaults()` used for handlers where no kwargs are provided?
- [ ] Is `bind_kwargs_with_map()` used for handlers where kwargs are provided?
- [ ] Are new test cases added for the modified behavior?
- [ ] Does the change apply consistently to ALL relevant handlers?

## Historical Issues

| Issue | Problem | Fix |
|-------|---------|-----|
| #2247 | `Call` bound kwargs varargs to `Nothing` instead of empty `Pairs` | PR #2268 |
| #2269 | `CallWithSplat` didn't bind kwargs at all | PR #2396 |
| #2397 | Duplicated kwargs logic across handlers | PR #2398 (shared helpers) |

## CallTypedDispatch (Issue #2587)

`CallTypedDispatch` handles Type{T} parametric dispatch (e.g., `promote_type(T, S)` where T,S are type parameters).

**Note**: The `sync_exec` dual execution path was **removed in PR #2796** (2026-02-12). There is now only a single execution path in `exec/call_dynamic_typed.rs`.

### Known Limitation

User-defined `promote_rule` methods called via `promote_type` may fail because
`promote_type` uses `CallTypedDispatch` with a frozen candidate list compiled
from Base methods. The runtime fallback in `call_dynamic_typed.rs` handles this,
but user-defined types may still hit edge cases.

## Type Parameter Binding (Issue #2468)

All call handlers extract type parameter bindings from arguments via `bind_type_params()`. This enables `where T` clause patterns to work with dynamic dispatch.

```rust
bind_type_params(&func, &args, &mut frame)
```

Handlers using this: `Call`, `CallWithKwargs`, `CallWithKwargsSplat`, `CallWithSplat`, `CallDynamic`, `CallDynamicOrBuiltin`, `IterateDynamic`, `CallDynamicBinary`, `CallDynamicBinaryBoth`, `CallDynamicBinaryNoFallback`, `CallGlobalRef`, `CallFunctionVariable`.

## Binary Method Dispatch Caching (Issue #2817)

Binary operator method dispatch results are cached by operand types in `binary_method_cache` (`vm/mod.rs`). This reduces repeated dispatch computation in hot loops with nary reduction patterns.

```rust
binary_method_cache: HashMap<BinaryDispatchKey, usize>
```

## Dict Parametric Mismatch Guard (Issue #2748)

All `CallDynamic*` handlers include a guard (`is_rust_dict_parametric_mismatch()`) that prevents Rust-backed `Value::Dict` from matching parametric `Dict{K,V}` methods. Pure Julia methods expecting `StructRef` would fail with "GetFieldByName: expected struct, got Dict" without this guard.

## Cache Alignment Invariant (Issue #2726)

When using precompiled Base cache, `function_infos` is initialized from the cache and user functions are appended at the end. This means cached bytecode contains `Call` instructions with indices that **must match** `function_infos` positions exactly.

### Invariant

For all Base function indices `i` (0 to `base_function_count - 1`):
```
all_functions[i].name == function_infos[i].name
```

A mismatch indicates that base function filtering has regressed — functions were reordered or incorrectly merged, causing `Call` instructions to invoke the wrong function at runtime.

### Enforcement

- **Debug assertion** in `compile/mod.rs`: After the function merging loop, a `#[cfg(debug_assertions)]` block verifies the invariant for all Base functions.
- **Exact signature matching**: Base function filtering (`is_same_function_signature`) must compare name, parameter types, and varargs status — not just the function name. This prevents false positives where a user function with the same name but different signature is incorrectly identified as a Base function duplicate.

### Prevention Test

`tests/fixtures/dispatch/test_user_base_coexist.jl` — Verifies that defining a user function with the same name as a Base function (e.g., `min`) does not break Base dispatch.

## Tfunc Metadata: Arity & Cost (Issue #3509)

Type-level dispatch during abstract interpretation is mediated by the
transfer-function registry in `subset_julia_vm_compile/src/compile/tfuncs/`. Each
registered tfunc is now stored as a `TransferRule` carrying:

- `min_arity: usize` and `max_arity: Option<usize>` — argument-count bounds.
- `cost: u32` — relative inference / inlining cost (`COST_CHEAP = 1`,
  `COST_MEDIUM = 10`, `COST_EXPENSIVE = 100`, `DEFAULT_COST = 10`).

This mirrors Julia's `add_tfunc(f, minarg, maxarg, tfunc, cost)` in
`julia/Compiler/src/tfuncs.jl`.

When `infer_return_type` is called, the registry first checks `argc` against
the rule's arity range. Out-of-range calls return `LatticeType::Top` and emit
an `arity mismatch` diagnostic, instead of silently propagating to the tfunc.
Convenience constructors:

- `register_exact(name, arity, cost, fn)` — exactly `arity` arguments.
- `register_ranged(name, min, max, cost, fn)` — `min..=max` (or `min..` if
  `max` is `None`).
- `register(name, fn)` — legacy shim with `min=0, max=None, cost=DEFAULT_COST`.

Migrated entries today: arithmetic (`+`, `-`, `*`, `/`, comparisons, `!`),
`isa`, `typeof`, `zero`, `one`, `typemin`, `typemax`, unary maths
(`sqrt`, `abs`, `sin`, `cos`, `exp`, `log`), `min`/`max`, `print`/`println`.
Remaining tfuncs continue to use the legacy shim and accept any arity; they
will be migrated incrementally.

## Return-Type Architecture: Three Channels (Issue #5425)

Return-type information for a function exists in three distinct channels that look
interchangeable but are NOT:

1. **`FunctionInfo.return_type`** — drives the compiled body's return instruction AND is
   read at runtime by reflection (`vm/builtins_reflection/mod.rs`). It only re-infers when
   `has_unknown_return_snapshot` (i.e. `return_type == Any` AND `return_julia_type ∈ {None, Any}`).
   **Setting this to `Any` breaks `Base.infer_return_type`**: the runtime reflection skips
   re-inference and returns `Any`. Keep it precise.
2. **`MethodSig.return_type`** — read by all compile-time call-type inference sites
   (`compile_call`, `infer_expr_type`, binary-op operand inference, assign stores).
   Setting this to `Any` makes a call-site dynamic everywhere at once.
3. **`infer_function_return_type_v2_with_arg_types`** (`compile/inference.rs`) — the
   call-site refinement used when `method.return_type == Any`. It only sees positional
   arg types (not kwargs) and may narrow back to the concrete signature. Must also return
   `Any` or it undoes changes to channel 2.

Caveat: `infer_function` and `infer_function_with_arg_types` share the cache key
`(fn, [])` for a kwarg-only function, so widening one silently corrupts the other.
For kwarg-inference fixes, widen `MethodSig.return_type` + the v2 helper + the compiled
body, and keep `FunctionInfo.return_type` precise. See PR #5464 for the reference fix.

## Function-Value Identity: Bare Name Canonicalization (Issues #10077, #10255, #10284)

**Design invariant**: a captured function *value*'s runtime type identity
(`typeof(f)` / `isa Function`) must depend only on which generic function it
is, never on the access path used to reach it. Upstream Julia's generic
function has exactly one canonical name (`nameof(f)`) regardless of whether
it was captured as `Module.func` (module-qualified) or `func`
(bare/imported) — both capture the identical singleton. `isa Function`
follows from the value's *kind* (`Value::Function` / a callable
`Value::BuiltinFunction`), never from a synthesized/qualified type name.

**The trap this prevents**: `method_tables` (and the runtime function table)
key module-scoped functions under a module-qualified string
(`"Pkg9992B.transform9992b"`, `"Base.sqrt"`) so that same-named functions
across modules don't collide (`Base.Iterators.flatten` vs.
`MacroTools.flatten`). That qualified string is an internal *lookup key*,
not the function's *identity*. Emitting it as the captured `FunctionValue`'s
`.name` (as the original module-qualified compile paths did) leaks an
implementation detail into `typeof(...)`/`isa Function`, and — because two
different access paths to the same function then produce two different
`.name` spellings — the same generic function ends up with two divergent
runtime identities depending on how it was captured (Issue #10077).

**The fix — separate "lookup key" from "captured identity"**:
`emit_function_value_named(display_name, lookup_name)` (`core_compiler.rs`)
resolves `method_tables` candidate indices through `lookup_name` (may be
qualified) but always emits the **bare** `display_name` as the value's
`.name`. `emit_function_value(name)` is the `(name, name)` degenerate case
for genuinely-unqualified access. Two compile-time sites and one runtime
reflection path route through this split:

- `compile_module_function_ref` (`compile/expr/call/module_call.rs`) — the
  general `Module.func` path and the Pure-Julia `Base.<fn>`-as-value path
  (Issue #8137, e.g. `Base.map`/`Base.sin`) call
  `emit_function_value_named(function, &qualified_name)`.
- `get_module_binding`'s dynamic `getfield(Module, :name)` runtime path
  (`vm/builtins_reflection/mod.rs`) re-keys a qualified-named function global
  to the bare field name before returning it, keeping its resolved
  candidate indices.

**Residual case (#10284) and its fix**: a small allowlist of Base functions —
`is_base_function` (`compile/base_functions.rs`: `sqrt`, `floor`, `ceil`,
`round`, `println`, …) plus operators routed through
`function_name_to_binary_op` (`+`, `-`, `*`, …) — are BuiltinOp/intrinsic-
backed and are **never** registered in `method_tables` under a genuine
`"Base.<fn>"` qualified key; Base's own Pure Julia extensions/methods for
these names live under the **bare** `<fn>` key instead (exactly like
`Base.map`/`Base.sin`'s general case). Calling
`emit_function_value_named(function, "Base.<fn>")` for these names always
found zero qualified candidates, so the empty-candidate fallback re-emitted
the qualified spelling itself (`PushFunction("Base.<fn>")`) — the one case
where the qualifier leaked into `.name` after #10077 fixed everything else.
The fix mirrors the `Base.<fn>`-as-value convention exactly: check for a
genuine qualified method-table entry first (future-proofing; none exists
today for this allowlist), otherwise resolve through the bare key via plain
`emit_function_value(function)`. Calling correctness is preserved because
`runtime_function_lookup_name` already strips any module prefix before
`BuiltinId::from_name`/intrinsic dispatch — the bare vs. qualified spelling
of a BuiltinOp-backed name was never load-bearing for *calling*, only for
the (buggy) captured-value identity.

**Trap this fix's own mechanism can hit — type names inside `is_base_function`**:
`is_base_function` is heterogeneous. A handful of its entries — `Int`,
`String`, `Char`, `Ref`, `IOBuffer` (and `MersenneTwister`) — are TYPE or
constructor names, listed there only so their CALL-path conversion behavior
(`Int(3.0)`, `String(chars)`) routes correctly; as a bare *value* (not
called), each one must resolve to the type object (`Base.Int isa Function`
is `false` upstream — it's a `DataType`). `compile_module_function_ref` must
check `is_builtin_type_name`/the struct-registry conditions **before** the
`is_base_function` branch — exactly the order the unqualified bare-identifier
path already uses in `compile/expr/mod.rs` (`is_builtin_type_name` before
`is_base_function`). Getting this order backwards is precisely how the
#10284 fix almost regressed: emitting a clean bare function-value name for
every `is_base_function` entry (the fix's whole point for `sqrt`/`floor`/…)
would, for `Int`/`String`/`Char`/`Ref`/`IOBuffer`, flip `Base.Int isa
Function` from `false` to `true` — the pre-fix qualified-name corruption had
coincidentally also produced `false`, masking the ordering bug until the fix
made the name clean.

**Rule of thumb when adding a new `Base.<fn>`-as-value path**: never emit a
module-qualified string as a `FunctionValue`/`ResolvedFunctionOperands`
`.name`. Use `emit_function_value_named(bare_name, lookup_key)` (or plain
`emit_function_value(bare_name)` when the lookup key is also bare) so the
qualifier only ever participates in method-table resolution, never in the
value's observable type.

## Call-Target Name Resolution Order: Local/Parameter Shadow Before Builtin Constructor (Issues #10146, #10268)

**Design invariant**: a bare (unqualified) call-target name resolves in
exactly one order — **local/parameter slot → captured vars → module
const/global → Base/global builtin** — for every surface that decides what a
call `name(args...)` means. A parameter or local variable named the same as
a builtin type-constructor name (`Int64`, `Float64`, `Bool`, `BigInt`,
`String`, …) shadows the global constructor throughout the function body,
exactly like any other Julia local binding (upstream lexical scoping).
`f(Float64) = Float64(2)` declares a one-argument function whose parameter
happens to be named `Float64`; `f(x -> x + 10)` must call the *parameter*
(the lambda, `== 12`), never the builtin `Float64(...)` conversion.

**The bug this closed (#10146 numeric case + #10268 general case)**: the
*main* stack compiler's `compile_call` (`compile/expr/call/mod.rs`) already
got this right — its `try_compile_callable_variable_call` resolution chain has
a trailing catch-all (`self.locals.contains_key(function) || self.captured_vars.contains(function)`)
that treats ANY local/captured name in call position as a callable-value
attempt before any global/builtin routing is considered. The bug lived in
two OTHER surfaces that independently decide what a call means, both of
which matched hardcoded builtin-constructor names without first checking
whether the name was actually a local/parameter binding for the CURRENT
function:

1. **Lazy runtime specializer** (`vm/specialize/expr.rs::compile_call`,
   fixed by PR #10417 / Issue #10146): a `"Int" | "Int64"` / `"Float64"`
   (and other) match arm fired unconditionally. A function with an untyped
   parameter is marked "specializable" (`spec_func_mapping`), so every call
   site emits `Instr::CallSpecialize` regardless of the stack-vs-SSA pipeline
   choice — this is why the bug reproduced identically under
   `SJULIA_SSA_PIPELINE=0`; the specializer sits below both pipelines. Fix
   (#10417): when `self.locals.contains_key(function)` at the top of
   `compile_call`, compile the callee as a callable variable
   (`compile_var(function)` + `Instr::CallFunctionVariable`) rather than
   taking a builtin-constructor arm — keeping the shadowed call correct AND
   specializable (splat/keyword forms bail to the ordinary body). A name that
   is not one of the function's own parameters/locals is unaffected —
   ordinary (non-shadowed) builtin-constructor calls keep the fast
   specialized path.
2. **Abstract-interpretation return-type engine**
   (`compile/abstract_interp/engine/mod.rs`, the `Expr::Call` arm of
   `infer_expr`, fixed by PR #10409 / Issue #10268 — the surface #10417 left
   untouched, so on post-#10417 main `f(String)=String(2); f(x->x+10)` still
   crashed): `infer_local_callable_call_return_type` already tries to
   resolve a local/captured callee's *precise* result when its bound lattice
   type is concrete enough, but when it returns `None` (an untyped parameter
   whose value isn't known at this call-site-independent, whole-function
   inference point) execution fell through to the transfer-function
   registry (`self.tfuncs.infer_return_type_with_context`), which maps
   well-known names like `String`/`Bool` to their builtin constructor's
   return type regardless of the shadow. For `Int64`/`Float64` this
   coincidentally stayed safe (the structural `Expr::Convert` rewrite gate,
   `numeric_convert_gate`, Issue #9803/PR #10139, independently declines to
   fold `Int64(x)`/`Float64(x)` when a param/type-param shadows the name,
   leaving a bare `Call` that infers `Any`) — but `String`/`Bool` have no
   such gate, so a shadowed `f(String) = String(2)` got a wrongly-declared
   `-> Str` return type. That is worse than a merely-wrong value: a caller
   compiled against the wrong declared type emitted the fast
   `PrintStrNoNewline` instruction, which then raised a runtime `Type error:
   expected String, got "Int64"` when the actual returned value (whatever
   the passed lambda produced) was not a genuine `Str`. Fix: right after
   `infer_local_callable_call_return_type` returns `None`, check
   `env.contains(function)` (the callee name is bound in the CURRENT
   function's type environment — i.e. it is a local/parameter) and return
   `LatticeType::Top` (`Any`) immediately, before the method-table /
   transfer-function fallbacks run. This lands on the same safe `Any`
   declared-return-type outcome the numeric `Expr::Convert` gate already
   produced for `Int64`/`Float64`, generalized to every name (not just the
   two the gate happens to cover).

**Deferred, separate root cause — where-clause TypeVar shadowing (Issue
#10407)**: a `where {Name}` type-parameter clause whose `Name` collides with
a builtin type name (`h(x::Float64) where {Float64} = Float64(2)`) is
**not** fixed by the above — it fails at method dispatch / TypeVar binding
(before the function body's calls are even reached), not at call-target name
resolution. Do not conflate the two: the non-colliding control case
`f(x::T) where {T} = T(2)` already works correctly in sjulia today.

**Rule of thumb when adding a new builtin-constructor name or a new
name-based fast path anywhere in the compiler**: never match a call's
`function` string against a hardcoded name list without first asking "could
this name be a local/parameter/captured binding for the function currently
being compiled/analyzed?" A `self.locals.contains_key(function)` /
`env.contains(function)` guard (or an equivalent shadow check) must run
first; a hardcoded name match is a `#10268`-class regression waiting to
happen.

## Array Constructor Direct-vs-Callable Parity Policy (Issues #10213, #10250)

**Design fact**: `Vector(src)` / `Array{T}(src)` / `Matrix(src)` have **two
independent entry paths** that can silently diverge:

- **Direct syntax** — `Vector(src)`, `Vector{T}(src)`, `Array{T}(undef, n)` —
  intercepted at compile time in `compile_array_constructor`
  (`compile/expr/collection.rs`), bypassing Base method dispatch entirely for
  performance.
- **First-class callable use** — `map(Vector, xs)`, `broadcast(Vector, xs)`,
  `Vector.(xs)`, `f = Vector; f(x)` — dispatches through ordinary Base methods
  in `subset_julia_vm/src/julia/base/array.jl` (`Vector(a::AbstractVector)`,
  `Matrix(A::AbstractMatrix)`, ...) at runtime, and separately through the
  VM's `Value::DataType`-as-callable dynamic-dispatch machinery
  (`vm/exec/call_function_variable.rs`) when the constructor value is
  parametric (`Vector{Int64}`).

Because the compiler intercept and the Base method surface are maintained
independently, nothing guarantees they agree. #10085 (PR #10193) found and
fixed exactly this: the direct path supported `Vector(::Vector)` and returned
the source object unchanged (no copy); the callable path had no matching
`Vector(::AbstractVector)` Base method at all, so `map(Vector, xs)` raised
`MethodError`.

**Policy**: whenever `compile_array_constructor` (or any future array/matrix
constructor intercept) grows a new source-type case, the audit fixture
`subset_julia_vm/tests/fixtures/array/ctor_direct_vs_callable_parity_10213.jl`
must be extended with the matching first-class-callable spelling
(`map(Ctor, [src])` at minimum), verified against `julia --startup-file=no`.
A compiler intercept case with no corresponding, parity-verified Base
callable method is exactly the class of bug #10213 was filed to prevent —
either add the Base method (mirroring upstream's
`AbstractArray{T,N}(A::AbstractArray{S,N}) where {T,N,S}` shape) or file a
tracked gap and document why the callable spelling is intentionally
unsupported, rather than letting the two paths silently disagree.

**Closure status (Issue #10250):** the compiler intercept remains as a
performance implementation detail, but it is no longer an unaudited semantic
owner. The Base/callable surface is authoritative and the following gates pin
every divergence discovered during the audit:

- `ctor_direct_vs_callable_parity_10213` compares values, concrete types, fresh
  copy identity, conversions, ranges, tuple errors, `Array`, `Vector`, and
  `Matrix` across direct, bound, `map`, `broadcast`, and dotted bare-callable
  spellings (#10085, #10213, #10405).
- `map_vector_ctor_outer_eltype_10187` separately pins the *outer* HOF result
  eltype that a single-result comparison cannot observe (#10187).
- `parametric_ctor_callable_parity_10502` and
  `dotted_parametric_constructor_10475` cover first-class/dotted parametric
  constructor targets and catchable dispatch failures (#10406, #10475).
- `scripts/metamorphic_equivalence.sh --lane direct_callable` compares value,
  result type, and exception class across direct/Base/bound/HOF lanes. Its
  two-sided divergence registry rejects both new divergences and stale
  allowlist entries; `--selftest` proves all comparators fire.

As of the #10250/#10272 closure audits, the linked #10213/#10187/#10085 and
follow-up #10405/#10406/#10475 issues are closed, the Julia/sjulia fixture pair
is green, and the metamorphic lane reports agreement with zero registered or
unregistered divergence. The outer-result rule is structural rather than a
`Vector`/`Matrix` name list: homogeneous concrete array-wrapper results recover
their `Array{T,N}` type through `array_wrapper_julia_type_resolved`; empty,
heterogeneous, non-array, and non-concrete results retain the conservative
fallback.

## Typed-Loop Inlining of `CallSpecializeI64Slots` (Issue #10439)

An **untyped** callee (`mygcd(a, b)`) reached from a typed loop through a
`CallSpecializeI64Slots` site used to keep the whole caller loop on the generic
interpreter: the typed-loop recognizer classified `CallSpecializeI64Slots` as an
unsupported instruction and rejected the loop (`SJULIA_TYPED_LOOP_DEBUG=1` shows
`reason=unsupported-instr:CallSpecializeI64Slots`). The callee body itself was
already specialized to a frame-less I64 body (`ExecutableBlock::I64Function` /
the euclidean fast path) via `record_i64_spec_dispatch` + `specialization_i64_cache`
(Issue #8167), but the *caller* loop paid per-iteration main-dispatch overhead
for the surrounding comparisons and counters. For `benchmarks/calc_pi_n5000.jl`
(untyped `mygcd`) that was ~2–8× slower than the fully-typed twin.

The recognizer (`try_predecode_typed_ops_range`, loop mode only —
`function_params.is_none()`) now accepts `CallSpecializeI64Slots` /
`CallSpecializeInboundsI64Slots` and emits `TypedLoopOp::CallSpecializeI64Function`,
recording only `(spec_func_index, arg_count)` in `TypedLoopBlock::specialize_callees`.

### Why the callee body is resolved at run time, not predecode time

The callee's I64 body is a **runtime** specialization: it is appended to
`self.code` lazily on the callee's first call, which happens *while the caller
loop executes*, i.e. **after** the caller loop was predecoded. So the callee
entry does not exist when the loop is recognized, and it cannot be predecoded
into `TypedLoopBlock::i64_callees` the way a `CallResolvedI64Slots` callee (a
static resolved method body) is (Issue #10309).

Instead, `execute_typed_loop_block` — which runs with `&mut self` because
`try_execute_executable_block` hands it a **cloned** block — resolves each
`specialize_callees` entry against the *live* `specialization_i64_cache`
immediately before running the typed ops (`resolve_specialize_i64_callee`),
cloning the predecoded `I64FunctionBlock` into a per-execution scratch vec that
is lent to the static `run_typed_ops_core` alongside `self.rng`.

### Correctness / bail policy

- The resolved body is the **same** predecoded I64 body the generic
  `CallSpecializeI64Slots` hit path runs (`try_execute_i64_function_call_i64_args`),
  so the result is bit-for-bit identical, including Julia's wrapping
  `+ - * abs` and truncated `%`. The `typemin % -1` / divide-by-zero cases make
  `checked_i64_rem` return `None`, which bails.
- A **miss** — callee not yet specialized (typically the loop's very first
  entry, before the callee's first call) or its body not I64-decodable — makes
  `execute_typed_loop_block` return `NotExecuted`, so the interpreter runs the
  block generically from its header. The typed-op core mutates only local
  `TypedOpsState`; the frame is untouched until a clean completion, so a bail
  never double-applies *frame* side effects (`TypedOpsOutcome::Bail`).
- **Side-effect transactionality guard (generalized by Issue #10504).** A
  `CallSpecializeI64Function` op can also bail *mid-block* at run time when its
  callee body returns `None` (`checked_i64_rem` on `MIN % -1` / `% 0`) — and so
  can every other data-dependent bail-capable op: `ModI64` / `LoadModI64Slot`
  directly, `IndexLoad*` on out-of-bounds / element-type mismatch, and
  `CallI64Function` / `CallF64Function` when the callee's execution bails.
  Unlike frame slots, `RandF64` advances the RNG and `IndexStore*` mutates the
  array heap **in place** during typed execution, so a bail that *follows* such
  an op would double-apply it on the generic re-run (`typemin % -1` bails
  *silently* — upstream yields 0 with no error — so the divergence is
  observable wrong output, e.g. a re-drawn RNG stream). The recognizer
  therefore rejects any block that mixes a bail-capable op
  (`typed_loop_op_can_bail_on_data`) with an in-place side-effecting op
  (`typed_loop_op_is_in_place_side_effect`) — the whole loop stays generic,
  which is transactionally correct. This subsumes the earlier
  specialize-call-only (#10439) and `IndexLoad*`-only (#10104) guards and
  mirrors the scalar-function predecoder's `RandF64` rejection (PR #9733). The
  hot targets (coprime-π's `mygcd` inner loop, Monte-Carlo rand loops, IFS
  LCG loops) each contain at most one of the two classes, so they are
  unaffected. Regressions:
  `typed_loop_rejects_call_specialize_i64_with_rand_side_effect_10439`,
  `typed_loop_rejects_mod_i64_with_rand_side_effect_10504`,
  `typed_loop_mod_bail_after_rand_draw_stays_generic_10504`.
- **Cache invalidation is inherited, not re-implemented.** The block stores only
  `(spec_func_index, arg_count)` and re-reads the live cache each entry, so a
  method redefinition that clears `specialization_i64_cache` /
  `i64_function_cache` (`clear_runtime_caches`, Issue #8453) simply produces a
  miss → generic run → re-specialize → re-populate. No stale callee body is ever
  cached inside the loop block.

### F64 mirror (Issue #10491)

The Float64 mirror is implemented with the same architecture plus two
F64-specific pieces:

- **`CallSpecializeF64Slots` / `CallSpecializeInboundsF64Slots`** — produced by
  the peephole fusion of a `LoadSlotF64` run feeding a `CallSpecialize` with a
  matching argument count (`try_fuse_f64_slot_arg_call`; mixed I64/F64 argument
  runs stay unfused). The handler (`execute_call_specialize_f64_slots`) probes
  `specialization_f64_cache` / `specialization_f64_fast_cache` (recorded by
  `record_f64_spec_dispatch` on the first all-`F64` call), mirroring the I64
  flow.
- **Specialized-body slot typing** (the piece the I64 path never needed):
  `install_specialized_body` used to slotize the runtime-specialized bytecode
  against the *fallback's* slot types, which tag every arg-dependent local
  `unknown` — so an untyped F64 helper's body kept generic `LoadSlot`/`StoreSlot`
  and no frame-less predecoder could accept it. It now derives the tags from
  the **specialized** argument types plus the specialized body's own typed
  name-based stores (`slot::build_specialized_slot_types`, conflict-poisoning
  merge identical to compile time; simple positional shapes only).
- **Mixed-type callee resolution.** An all-`F64`-argument helper usually
  carries an I64 loop counter, so a pure `F64FunctionBlock` cannot represent
  it. `TypedLoopOp::CallSpecializeF64Function` resolves through
  `ResolvedSpecF64Callee`: the pure-F64 block when possible, otherwise the
  mixed-type frame-less `TypedScalarFunctionBlock` (Issue #9693), executed by
  `run_typed_scalar_block_with_f64_args` (recursion into `run_typed_ops_core`;
  bounded because function-mode predecode never inlines specialize sites). A
  non-`F64` return value bails — the fused caller site is typed F64.

Result on the Issue #10491 MWE (`scan(2000)`, nested loop, untyped `fstep`):
the untyped-helper form runs the whole caller loop natively
(`ExecutableBlock::TypedLoopCallSpecializeF64Function`) at ~0.38 s vs ~1.84 s
for the fully-typed twin — the twin's `CallResolved` site cannot inline a
mixed-type callee into a typed loop yet (`typed_loop_f64_call_op` requires a
pure-F64 body; see Issue #10542 follow-up). Typed scalar *function* /
broadcast blocks never inline these sites: emission is gated to loop mode, and
those call paths pass empty resolved-callee slices.

### Block-entry callee inlining (Issue #10516)

`execute_typed_loop_block` splices small resolved I64 callee bodies directly
into the op stream before running the typed ops
(`try_inline_i64_callees_into_typed_ops`), eliminating the per-call argument
copy, local re-initialization, and nested mini-interpreter dispatch of
`execute_i64_function_block` (~25M calls per `calc_pi_n5000` run; cold CLI
~3.6 s → ~2.8 s). Entry-time splicing covers BOTH `CallI64Function` and
`CallSpecializeI64Function` sites and inherits the specialize path's
cache-invalidation contract (the callee is re-resolved from the live cache on
every entry). Eligibility: callee ≤ 32 ops, no nested callees, translatable
ops only, exact `arg_count` i64 depth at the site (linear depth simulation),
and the fresh locals fit `TYPED_LOOP_SLOT_CAP`. Each splice starts with
`UninitI64Slot` for every non-param callee local, reproducing the frame-less
executor's per-call `local_init` reset so load-before-store paths bail
identically. The #10504 guard already rejected any block mixing these call
sites with in-place side effects, so a bail from an inlined body never
double-applies an effect.

## Related Documentation

- `CLAUDE.md` - Shared Function Pattern
- `TYPE_SYSTEM.md` - Type parameter handling
- `LOWERING.md` - How kwargs are represented in IR
