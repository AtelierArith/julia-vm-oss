# Scalar Function Block IR (frame-less i64/f64 function inlining)

Generalized scalar-function-block IR that lets small, wholly-scalar user
functions (`f(x::Int64)::Int64`, `sq(x::Float64)::Float64`, …) be **predecoded
once** into a compact native op list and executed **without a VM frame** — no
argument-slot binding, no per-instruction dispatch, no return routing. This is
the machinery behind the `CallI64Function` / `CallF64Function` typed-loop ops and
the direct-call fast paths that keep gcd-heavy integer kernels (coprime π) and
Float64 kernels (function-inline Mandelbrot) off the slow frame path.

- History: `I64FunctionBlock` landed in PR #10309 (Issue #10309); the Float64
  mirror `F64FunctionBlock` landed in PR #10426 with code sharing intentionally
  deferred. Issue **#10427** extracted the common machinery into one generic IR.
- Location: `subset_julia_vm_vm/src/vm/executable.rs`.

## The abstraction (Issue #10427)

Everything structural is written **once**, generic over the scalar element type
`S` (`i64` or `f64`); the type-specific semantics live behind one trait.

| Item | Role |
|------|------|
| `ScalarKind` (trait) | Supplies the type-specific pieces: `add`/`sub`/`mul`/`div`/`neg`/`abs`, `checked_rem`, `eval_relation`, and the two profiler tags (`BLOCK_EVENT`, `NESTED_CALL_EVENT`). |
| `I64Kind`, `F64Kind` | Zero-sized marker types implementing `ScalarKind` (`type Scalar = i64` / `f64`). |
| `ScalarFunctionSlot` | One local slot `{ slot, param_index }`. Not parameterized (identical for every scalar type). |
| `ScalarFunctionOp<S>` | The op enum: the **union** of the ops both predecoders emit. Generic only in `Push(S)` / `AddConstSlot(usize, S)` payloads. |
| `ScalarFunctionBlock<S>` | `{ slots, ops, callees }` — a predecoded block plus its frame-less callees. |
| `ScalarFunctionBuilder<'a, S>` | Predecode-time builder: slot dedup + param binding + simulated operand/bool stack-depth guards. |
| `ScalarRelation` | The single 6-variant ordered/equality relation, shared with the typed-loop op set. |
| `execute_scalar_function_block::<K>` | The **single** mini-interpreter loop. Monomorphizes to one i64 and one f64 interpreter. |

### What is shared vs. what stays type-specific

**Shared (written once):** the slot/param model, the builder, the mini-interpreter
loop (control flow, operand/bool stack discipline, slot binding, nested-call
dispatch, bail-to-frame guards), the jump-target remap (`remap_scalar_function_op_targets`),
the target-index helper (`scalar_function_target`), and the fixed-capacity stack
primitives (`push_stack` / `pop_stack` / `pop2_stack`).

**Type-specific (kept local, per the issue):**

- **Predecode instruction coverage** — the two recognizers
  (`try_predecode_i64_function_inner`, `try_predecode_f64_function_inner`) map
  genuinely different bytecode instructions (i64 has loop-induction ops
  `IncVarI64Slot`/`AddConstI64SlotAndJumpIfLe`/…; f64 has `DivF64`/`NegF64`/the
  `JumpIfNot*F64` family and the `PushI64 → f64` widening). Each emits only the
  ops meaningful for its type.
- **Arithmetic semantics** — I64 uses wrapping overflow and a checked Euclidean
  modulo (`checked_rem` bails on `÷0` / `i64::MIN % -1`); F64 uses IEEE division
  and NaN-aware ordered comparison.
- **Per-type base-unary calls** — `abs` (`i64` strict-signature check vs. `f64`).
- **Post-return compare-branch fusion** (`try_consume_i64_eq_branch`,
  `try_consume_f64_eq_branch`) and the two runtime specialization caches, which
  key on distinct instruction shapes and block types.

### Op variant mapping (old → generic)

The union enum `ScalarFunctionOp<S>` replaces the old `I64FunctionOp` /
`F64FunctionOp`. Neutral names: `Push`, `LoadSlot`, `StoreSlot`, `Add`, `Sub`,
`Mul`, `Div` (f64-only), `Neg` (f64-only), `Abs`, `Rem` (i64-only),
`LoadAddSlot`/`LoadSubSlot`/`LoadMulSlot`, `LoadDivSlot` (f64-only),
`LoadRemSlot` (i64-only), `IncSlot`/`DecSlot`/`AddConstSlot`/
`AddConstSlotAndJumpIfLe` (i64-only), `Call`, `Cmp`, `JumpIfZero`, `JumpIf`,
`JumpIfSlots`, `Jump`, `Return`. Variants an individual predecoder never emits
are simply never present in that type's blocks; the interpreter arm for them
still monomorphizes but is never reached.

## Behavior and performance preservation

- **Behavior:** the generic interpreter's arms are per-op identical to the two
  originals. F64's `eval_ordered_f64_relation` (partial-`cmp`) and the direct
  `<`/`>`/… operators used by `eval_f64_relation` are equal for `f64`, so folding
  the `JumpIfSlots` arm onto `K::eval_relation` is a no-op; the i64
  `AddConstSlotAndJumpIfLe` `<=` becomes `K::eval_relation(.., Le)`, identical.
- **Performance:** `K: ScalarKind` methods are `#[inline(always)]` and trivial,
  so `execute_scalar_function_block::<I64Kind>` / `::<F64Kind>` monomorphize to
  the same code the two hand-written interpreters produced. Verified against the
  coprime-π (`calc_pi_benchmark`) and function-inline Mandelbrot
  (`f64_mandelbrot_function_inline_benchmark`) VM benches — no regression.

## Back-compat aliases

To keep the blast radius inside `executable.rs`, the old names remain as
aliases: `pub(crate) type I64FunctionBlock = ScalarFunctionBlock<i64>`,
`pub type F64FunctionBlock = ScalarFunctionBlock<f64>`,
`pub type F64FunctionOp = ScalarFunctionOp<f64>`,
`pub type F64FunctionSlot = ScalarFunctionSlot`,
`pub type F64FunctionBuilder<'a> = ScalarFunctionBuilder<'a, f64>`, and
`pub(crate) type I64Relation` / `pub type F64Relation = ScalarRelation`. The
`#[doc(hidden)]` F64 test API (`Vm::execute_f64_function_block`,
`tests/f64_function_block_tests.rs`) is preserved through these aliases; only the
op-variant spellings changed (`PushF64` → `Push`, `f64_slot` → `slot`, …).

## Adding another scalar kind (deferred — f32/bool)

Issue #10427 keeps scope to i64/f64 ("eventually f32/bool"). To add one:

1. Implement `ScalarKind` for a new marker type (its `Scalar`, arithmetic,
   `eval_relation`, and profiler tags).
2. Add a predecoder that maps the type's bytecode instructions to
   `ScalarFunctionOp<S>` (reuse `ScalarFunctionBuilder`, `scalar_function_target`,
   `remap_scalar_function_op_targets`).
3. Add a thin `execute_*_function_block` wrapper delegating to
   `execute_scalar_function_block::<NewKind>`, plus a runtime cache + dispatch
   fast path if the type participates in typed loops / direct calls.

No change to the interpreter, block, op, builder, or remap is required.
