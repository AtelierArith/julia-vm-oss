# F64FunctionBlock and Inline CallF64Function into Typed Loops

**Date:** 2026-07-10  
**Status:** Design approved, ready for implementation plan  
**Related:** Issue #10309 (typed-loop I64 call inline), PR #10358

## Background

`subset_julia_vm` already has a specialized execution path for `Int64`-typed loops and small `Int64` functions:

- `I64FunctionBlock` / `I64FunctionOp` in `subset_julia_vm_vm/src/vm/executable.rs`
- `TypedLoopOp::CallI64Function` for invoking pre-decoded I64 callees inside typed loops
- `try_consume_i64_eq_branch` for fusing a returned `i64` with an immediate `==` / `!=` branch

PR #10358 extended this so that `CallI64Function` can be inlined directly into typed-loop bodies, avoiding the frame setup overhead for helpers such as `mygcd` in the coprime-π benchmark.

The same optimization does not exist for `Float64`. User-defined functions such as

```julia
f(x::Float64)::Float64 = x * 2.0 + 1.0
```

still execute through the generic dynamic-call path even when called from a `Float64`-typed loop, paying dispatch and boxing costs.

## Goal

Add a `Float64` mirror of the I64 specialized function-block infrastructure and inline `Float64` function calls into typed loops, yielding comparable speed-ups for F64-heavy kernels.

## Non-Goals

- Do not refactor the existing I64 path to share code yet; accept duplication to keep the change reviewable.
- Do not handle `ComplexF64` or other scalar types; they already have separate fast paths (e.g. `ComplexF64MandelbrotEscapeLoopBlock`).
- Do not change Julia source semantics; all optimizations must fall back gracefully when preconditions are not met.

## Design

### 1. New IR types (`subset_julia_vm_vm/src/vm/executable.rs`)

Add the following types alongside their I64 counterparts:

- `F64FunctionBlock { slots, ops, callees }`
- `F64FunctionSlot { slot, param_index }`
- `F64FunctionBuilder<'a>`
- `F64FunctionOp` with variants:
  - `PushF64(f64)`
  - `LoadF64Slot(usize)`, `StoreF64Slot(usize)`
  - `AddF64`, `SubF64`, `MulF64`, `DivF64`, `NegF64`
  - `LoadAddF64Slot(usize)`, `LoadSubF64Slot(usize)`, `LoadMulF64Slot(usize)`, `LoadDivF64Slot(usize)`
  - `CallF64Function(callee_index, arg_count)`
  - `CmpF64(F64Relation)`
  - `JumpIfF64(F64Relation, target)`
  - `Jump(target)`
  - `ReturnF64`

`F64Relation` mirrors `I64Relation` (`Eq`, `Ne`, `Lt`, `Le`, `Gt`, `Ge`).

### 2. Predecode

Add `try_predecode_f64_function` in `subset_julia_vm_vm/src/vm/executable.rs` (adjacent to `try_predecode_i64_function`) that recognizes functions whose parameters and return type are all `Float64` and whose body consists only of:

- F64 arithmetic (`+`, `-`, `*`, `/`, unary `-`)
- F64 comparisons
- Calls to other predecoded F64 functions (up to a nesting cap)
- `abs` / `sqrt` if deemed worthwhile

If predecode fails, execution falls back to the existing generic path.

### 3. Typed-loop integration

- Add a `f64_callees: Vec<F64FunctionBlock>` field to `TypedLoopBlock`.
- Add `TypedLoopOp::CallF64Function(callee_index, arg_count)`.
- During typed-loop execution, when this op is hit, run the callee inline using a local F64 mini-stack and slots, then continue the loop.

### 4. Execution engine

Implement `execute_f64_function_block` analogous to `execute_i64_function_block`. It operates on:

- A small fixed-size `f64` operand stack
- A `locals` array indexed by `F64FunctionSlot`
- A simple op-code interpreter loop

### 5. Post-return branch fusion

Add `try_consume_f64_eq_branch` (and ordering variants) so that patterns such as

```text
CallF64Function(...)
PushF64(1.0)
EqF64
JumpIfZero(target)
```

are consumed without pushing the result onto the main VM stack.

**Important:** F64 comparison must respect Julia semantics for `NaN`:

- `NaN == x` is `false`
- `NaN != x` is `true`
- Ordered comparisons (`<`, `<=`, `>`, `>=`) involving `NaN` are `false`

This differs from I64, where comparison is always total.

### 6. Call-dispatch fast path

In `subset_julia_vm_vm/src/vm/exec/call.rs`, add fast paths for calls that resolve to a predecoded F64 block, e.g. `CallDirectFastF64FunctionHit`, mirroring the I64 variants. After the callee returns an `f64`, attempt `try_consume_f64_eq_branch` before pushing to the main stack.

### 7. Jump-target remapping

Implement `remap_f64_function_op_targets` to adjust jump targets when a block is cloned or inlined, mirroring `remap_i64_function_op_targets`.

### 8. Validation / guard conditions

Reject predecode or inline expansion when:

- The function body contains unsupported constructs (closures, global access, exceptions, non-F64 literals).
- The caller loop is not F64-typed for the relevant arguments.
- Nesting depth or callee count exceeds a configured cap.
- Jump targets would cross loop boundaries in invalid ways.

## Testing Plan

1. **Parity fixtures** under `subset_julia_vm/tests/fixtures/`:
   - `f(x::Float64)::Float64 = x * 2.0 + 1.0` called from a typed loop
   - Nested F64 callees
   - Compare-branch patterns: `==`, `!=`, `<`, `<=`, `>`, `>=`
   - Edge cases: `NaN`, `Inf`, `-0.0`

2. **Unit tests** for `F64FunctionBuilder` stack/slot validation.

3. **Profiler-gated tests** (when the `profiling` feature is enabled) verifying that events such as `ExecutableBlock::F64Function` and `ExecutableBlock::F64FunctionCompareBranch` fire.

4. **Benchmark**: add a Criterion case or CLI fixture for an F64-heavy kernel (e.g. sum of squares, a simple explicit ODE step, or a scalar map) to demonstrate the speed-up versus the generic path.

## Risks and Mitigations

| Risk | Mitigation |
|------|------------|
| Code duplication with I64 path | Document the intentional mirroring; consider a follow-up Issue for generic abstraction after this path is proven. |
| F64 comparison semantics (NaN) | Centralize comparison logic and add parity fixtures covering NaN/Inf. |
| Division by zero / Inf behavior | Keep Julia semantics by using Rust `f64` arithmetic directly; add edge-case fixtures. |
| Increased compile time / binary size | Keep the new code behind the same cfg/features as I64; gate on typed-loop detection. |
| Regression in existing I64 path | Do not modify I64 code paths except for shared helpers if absolutely necessary. |

## Success Criteria

- The parity fixtures pass under both `sjulia` and upstream `julia`.
- `cargo clippy --all-targets -- -D warnings` and `cargo fmt --check` are clean.
- A benchmark shows measurable speed-up for an F64 kernel compared to the pre-change generic path.
- No regressions in the full `cargo nextest run --release` suite.

## Open Questions / Follow-ups

- Should `abs` / `sqrt` be included in the initial op set, or deferred to a second pass?
- After this lands, should we open a refactor Issue to generalize I64/F64 scalar function blocks?
