# Design: Retire the ComplexF64 resolved-call fast path (Issue #10530)

## Status

Approved — ready for implementation planning.

## Background

`subset_julia_vm/src/julia/base/complex.jl` already defines `Complex{T<:Real}` arithmetic in pure Julia, e.g.:

```julia
function Base.:*(z::Complex{T}, w::Complex{S}) where {T<:Real, S<:Real}
    r = real(z) * real(w) - imag(z) * imag(w)
    i = real(z) * imag(w) + imag(z) * real(w)
    return Complex{typeof(r)}(r, i)
end
```

The VM previously kept a Rust-level fast path for `ComplexF64` calls on the abstract/`::Complex` route:

- `vm/exec/call.rs::try_complex_f64_resolved_call_fast_path` intercepted direct calls to `*` and `abs2` when the operands were `ComplexF64` and executed them directly in Rust.
- `vm/exec/binary_both.rs::try_complex_f64_binary_op` handles non-SROA'd dynamic binary ops.
- `vm/dynamic_ops/mod.rs::try_complex_f64_int_pow` handles `ComplexF64^Integer`.

This PR removes `try_complex_f64_resolved_call_fast_path` and retires the `abs2` side: `Base.abs2(::ComplexF64)` already runs frame-lessly through the normal `execute_direct_call_fast` → `try_execute_typed_scalar_function_call` path because the concrete method `abs2(z::Struct(104)) -> F64` is not a runtime-specialization candidate. The `*`/`+` interception is also removed; those operations fall back to the remaining `CallDynamicBinaryBoth` fast path and normal frame dispatch until their generic bodies can be predecoded (follow-up Issue).

## Goal

Retire the `abs2` side of `try_complex_f64_resolved_call_fast_path` by making the VM execute `Base.abs2(::ComplexF64)` through the pure-Julia `TypedScalarFunctionBlock` path. Also remove the resolved-call interception for `*`/`+`; these fall back to the remaining `CallDynamicBinaryBoth` fast path and normal frame dispatch until their generic bodies can be predecoded.

## Scope

### In scope for this PR

- Delete `try_complex_f64_resolved_call_fast_path` and `try_complex_f64_abs2`.
- Confirm that `Base.abs2(::ComplexF64)` reaches `try_execute_typed_scalar_function_call` through the normal direct-call fast path and runs frame-lessly.
- Add regression tests and benchmark results.
- File a follow-up Issue for `Base.*/Base.+` generic-body predecode.

### Out of scope for this PR

- Lowering/predecode changes needed to make the generic `Base.*/Base.+` bodies predecodable.
- `try_complex_f64_binary_op` (non-SROA'd dynamic binary ops).
- `try_complex_f64_int_pow` (`ComplexF64^Integer`).
- Deleting `complex_fastpath_gate.rs` (still needed by the remaining fast paths).

## Architecture

`execute_direct_call_fast` already runs small typed functions through `try_execute_typed_scalar_function_call`. The concrete base `abs2(::Complex{Float64})` method (`abs2(z::Struct(104)) -> F64`) is not a runtime-specialization candidate, so it reaches the typed-scalar path through the normal guard and predecodes to a frame-less block. The generic `Base.*`/`Base.+` methods remain excluded as runtime-specialization candidates; they fall back to normal frame dispatch and the remaining `CallDynamicBinaryBoth` fast path.

```text
execute_direct_call_fast(func_index, arg_count)
  │
  ├─ existing guards (is_generated, vararg, kwparams, type_params,
  │    runtime-specialization candidate, arity match)
  ├─ try i64 scalar function
  ├─ try f64 scalar function
  ├─ try typed scalar function (existing)
  │     │
  │     ├─ predecode Base.abs2(::Complex{Float64}) body
  │     ├─ bind ComplexF64 args as slot pair
  │     └─ execute frame-lessly → Value::F64
  │
  └─ fall through to normal frame dispatch
```

## Components & Data Flow

1. **Remove the resolved-call fast path** (`call.rs`).
   - Delete `try_complex_f64_resolved_call_fast_path` and its call site in `execute_call`.

2. **Remove the unused helper** (`binary_both.rs`).
   - Delete `try_complex_f64_abs2`; it was only used by the resolved-call fast path.

3. **Argument binding / block execution** (`executable.rs`).
   - The existing `try_execute_typed_scalar_function_call` is reached for the concrete `abs2(z::Struct(104)) -> F64` method through the normal direct-call fast path.
   - It predecodes the method body into a `TypedScalarFunctionBlock` (cached per entry IP).
   - `bind_typed_function_param` extracts `(re, im)` from the `ComplexF64` argument and binds to `TypedFunctionParamBinding::ComplexF64`; non-ComplexF64 arguments cause a safe fallback.
   - `run_typed_ops_core` executes the body frame-lessly and returns `Value::F64`.

## Error Handling / Fallback

- If any precondition fails, return `Ok(None)` so the normal dispatch path runs.
- If argument extraction fails, push the popped values back and return `Ok(None)`.
- If block construction bails, return `Ok(None)`.
- The existing `try_or_handle`/`DispatchAction::Continue` protocol remains unchanged.

## Testing & Acceptance Criteria

1. **Correctness parity**:
   - `cargo bench -p subset_julia_vm --bench vm_complex_dynamic_9198_benchmark` must still pass output parity between `/with_fastpath` and `/without_fastpath` for `dyn_binary`, `array_sum`, `dyn_pow`, and `mandelbrot_guard`.
   - Full release suite: `timeout 1800 cargo nextest run --release` green.

2. **Performance**:
   - `benchmarks/mandelbrot_bench_for.jl` run as untyped (`mandel_point(c, maxiter)` without `::ComplexF64`) should improve over the current ~38 s baseline (measured on the same machine). If the pure-Julia route does not beat the Rust fast path yet, the regression must be small and documented; the fast path is not retired until the numbers support it.
   - `cargo bench -p subset_julia_vm --bench vm_complex_dynamic_9198_benchmark` must show that `mandelbrot_guard/without_fastpath` stays within noise of `mandelbrot_guard/with_fastpath` (already true today).

3. **Regression tests**:
   - Add a test in `mandelbrot_tests.rs` that runs the Mandelbrot kernel with an abstract `::Complex` parameter and asserts it hits the typed-loop / frame-less path (e.g., by inspecting bytecode or by performance assertion).
   - Add a test that `abs2(::ComplexF64)` on the abstract route produces the same result as the typed route.

## Open Questions / Risks

1. **Generic `*`/`+` body predecode**.
   - `Base.:*`/`Base.:+` generic bodies call `real`/`imag` via `CallDynamic` and construct the result with `CallBuiltin(TypeOf)` / `NewDynamicParametricStruct`. These are not accepted by `try_predecode_typed_scalar_function`. Until lowering/predecode is extended, those methods fall back to normal frame dispatch and the remaining `CallDynamicBinaryBoth` fast path.

2. **Constructor result type**.
   - `Base.:*` returns `Complex{typeof(r)}`. For `ComplexF64 * ComplexF64`, `typeof(r)` is `Float64`, so the result type is `ComplexF64`. A future typed-scalar block for `*` would return a `Value::Struct`; the caller's SROA logic must still unbox it into a slot pair.

## Related Issues

- #10530 — umbrella: execute pure-Julia Complex methods fast enough to retire Rust fast paths.
- #10532 — `ComplexF64MandelbrotEscapeLoopBlock` retirement (already done in PR #10619 / #10310).
- #9198 S6 — A/B measurement gate for Complex fast paths.
- #9693 — `TypedScalarFunctionBlock` (frame-less scalar function IR).
