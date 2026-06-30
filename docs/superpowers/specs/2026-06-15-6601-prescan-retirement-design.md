# Design — #6601: Retire the function-body slot-typing pre-scan (2-pass incremental)

- **Issue**: #6601 ("[#5922] pre-scan 退役(1/3): 関数本体/inner ctor/main のスロット型を 2パス化")
- **Parent epic**: #5922 (推論ドライバ一本化・二重推論解消)
- **Date**: 2026-06-15
- **Approach chosen**: 2-pass incremental (over the alternative "lazy slot allocation")

## Problem

Before emitting bytecode for a function body / inner constructor / `main`, the
compiler runs a statement-level **pre-scan**
(`compile/inference.rs::collect_local_types_with_mixed_tracking` →
`collect_local_types_with_mixed_tracking_impl`) that pre-populates two pieces of
state read during codegen:

1. `compiler.locals` — the **whole-body widened slot type** for every assigned
   local, read as `target_ty` *before* compiling each first `Store`
   (`compile/stmt.rs`, the `Assign` arm). This is what types a forward reference
   correctly (`s = 0; s = s + 1.5`): the slot decision at the first store already
   reflects later assignments.
2. `compiler.mixed_type_vars` — the set of locals forced to dynamic
   (`StoreAny`/`LoadAny`) slots.

This pre-scan duplicates inference logic that the shared abstract-interpretation
`InferenceEngine` also performs, which is exactly the double-inference #5922
exists to remove. After #6602 (loop-var typing → engine injection) and #6603
(globals → engine injection), this function-body consumer is the **sole
remaining** pre-scan consumer.

The shared engine's forward refinement refines *expression results at call
sites*, not the pre-store slot decision, so the consumer cannot be deleted
outright. Retiring it requires a structural change. Two behavior-preserving
shapes were documented in `docs/vm/TYPE_INFERENCE_COMPLETE.md`; we take the
**2-pass incremental** one.

## Key insight (why 2-pass is clean)

The Assign arm of `collect_local_types_with_mixed_tracking_impl` has three
**independent** pieces (see `compile/inference.rs`, Assign arm):

1. **`ty` computation** — `Expr::Literal` already routes through
   `abstract_interp::local_authority::literal_assignment_value_type` (an
   engine-equivalent mapping, #5922 partial retirement); **all other RHS
   classes** still call legacy `infer_value_type_with_structs`.
2. **`mixed_type_vars` rules** — Rule A (direct-literal F64↔I64 reassignment) and
   Rule B (#3535 incompatible non-numeric reassignment), computed purely from
   `old_ty`, `ty`, and the **syntactic** `is_direct_literal` check
   (`matches!(value, Expr::Literal(Int|Float|Float32|Float16))`).
3. **`widen_type(old, ty)`** — the slot join.

Pieces 2 and 3 read only `old_ty` / `ty` / RHS-syntax. They are **independent of
which inference path produced `ty`.** This is the linchpin: the documented "crux"
(the engine's `join` does not distinguish "direct literal F64/I64" from "compound
numeric widening") is already solved by keeping `is_direct_literal` as a syntactic
check **in the driver**. The migration only has to swap the `ty` computation per
RHS class while keeping `ty`'s *value* identical — proven per class by a pin test.

## Architecture

Keep `collect_local_types_with_mixed_tracking_impl` as **pass 1** (the driver).
It retains `widen_type` and both `mixed_type_vars` rules unchanged. The shared
`InferenceEngine` is *already* threaded in (`loop_inference_engine`, used today
for `For`/`ForEach`). Migrate the non-literal Assign-RHS `ty` computation, one
class at a time, from `infer_value_type_with_structs` onto
`engine.infer_expr_result(expr, &env)` → `bridge::lattice_to_value_type(...)` —
the exact path #6602 used for loop-variable typing.

Because pieces 2/3 are path-independent, **nothing about
`mixed_type_vars`/`widen_type` changes**; each slice only proves the new `ty`
equals the old `ty` for its RHS class.

## Migration slices (one PR each; per-slice pin test + full suite)

- **Slice 0 — scaffolding (no behavior change).** Extract an
  `assign_rhs_value_type(engine, env, value, locals, struct_table, global_types)`
  helper that *today still calls legacy*. Add a characterization harness
  `prescan_engine_equiv_<class>` comparing legacy vs engine `ValueType` over a
  fixture corpus, so each later slice flips one branch and the harness proves
  equivalence.
- **Slice 1 — already-equal classes.** RHS classes where engine ≡ legacy out of
  the box: resolved-local `Var`, concrete-numeric `BinaryOp`. Swap + pin.
- **Slice 2 — `Expr::FunctionRef`.** Legacy → `ValueType::Function`; engine
  produces `ConcreteType::Function`, which `bridge::lattice_to_value_type` maps to
  `ValueType::Any`. Fix in the bridge (`Function → ValueType::Function`) or a
  documented driver shim; pin + regression fixture.
- **Slice 3 — bare `pi` / `π` `Var`.** Legacy returns `F64` via `is_pi_name` when
  the name is not in `global_types`; the empty-table engine returns `Any`. Seed
  the engine env (or a documented driver shim) so they agree; pin + regression
  fixture.
- **Slice 4 — struct-aware classes.** `Expr::Call` / `Expr::Index` /
  `Expr::FieldAccess` / Complex-promoting `Expr::BinaryOp`. The legacy
  `infer_value_type_with_structs` carries struct-aware Complex promotion, a
  struct-preserving-function list, and array/field element typing reached through
  a different (tfunc-registry) engine path; equivalence must be proven
  class-by-class, **not assumed**. One sub-PR per class; extend engine/bridge
  where it under-approximates vs legacy; each full-suite-gated, each with a
  regression fixture.
- **Slice 5 — delete legacy.** Once `infer_value_type_with_structs` /
  `infer_value_type` have no callers for this consumer, delete them. The pre-scan
  body is now a thin engine driver; the two inference paths unify. Verify
  `base_exports_do_not_exceed_upstream` + full suite + aot.

## Hazards (verified while scoping #6601)

| RHS class | Legacy result | Empty-engine result | Resolution |
|---|---|---|---|
| `Expr::FunctionRef` | `ValueType::Function` | `ConcreteType::Function` → `Any` | bridge fix or driver shim (Slice 2) |
| bare `pi`/`π` `Var` | `F64` (`is_pi_name`) | `Any` | engine env seeding or shim (Slice 3) |
| `BinaryOp`/`Call`/`Index`/`FieldAccess` | struct-aware Complex promotion, struct-preserving-function list, array/field element typing | tfunc-registry path (may under-approximate) | prove + extend per class (Slice 4) |

The engine for the pre-scan loop is built with **no function table**
(`build_shared_inference_engine_empty`), so `resolve_callable_name` returns
`None` and plain unknown `Var`s agree with legacy at `Any`. Keep this constraint
in mind: the pin tests must use the same empty-table engine the driver uses.

## Load-bearing behavior to preserve (must stay green every slice)

The `prescan_*_issue_6601` characterization tests in `compile/inference.rs`:

| Sequence | Slot type | `mixed_type_vars`? | Why |
|----------|-----------|--------------------|-----|
| `s = 0; s = s + 1.5` | `Any` | no | `I64 ⊔ F64 → Any`; compound RHS not a direct literal |
| `s = 0.0; s = s + 1.5` | `F64` | no | stable numeric slot |
| `die = 7.0; die = 6` | `Any` | **yes** | direct F64/I64 literal reassignment → dynamic |
| `v = 1; v = "s"` | `Any` | **yes** | incompatible non-numeric (#4285/#3535) |
| `acc = 0; acc = acc / 2` | `Any` | no | compound numeric widening, not the direct-literal rule |

## Testing strategy

- Keep the full `prescan_*_issue_6601` table green at every slice.
- Each slice adds a per-class equivalence pin (`assert_eq!` legacy `ValueType`
  vs engine `ValueType` over representative RHS shapes, using the empty-table
  engine).
- Each slice runs the **full** `timeout 1800 cargo nextest run --release`
  (forward-reference fixtures span many categories; never `| tail` — CLAUDE.md).
- Slices 2–4 each add a Julia fixture regression test under
  `subset_julia_vm/tests/fixtures/` verified against upstream `julia` first.
- Slice 5 additionally runs `--features aot` and the
  `base_exports_do_not_exceed_upstream` check.

## Done criteria

- `infer_value_type` / `infer_value_type_with_structs` deleted (no callers for
  the function-body consumer).
- Pre-scan body purely engine-driven; `mixed_type_vars` contract unchanged and
  pinned.
- Full suite + aot green; `base_exports_do_not_exceed_upstream` unchanged.
- `docs/vm/TYPE_INFERENCE_COMPLETE.md` updated: the consumer is retired; record
  the final seam.

## Out of scope

- The "lazy slot allocation" alternative (removing the pass entirely) — rejected
  as higher blast radius; the 2-pass driver keeps `mixed_type_vars` reconstruction
  trivial.
- #6599 (ValueType→LatticeType view demotion) — separate epic, separate spec.
- Pre-scan parts 2/3 of the "1/3" sequence beyond the function-body consumer
  (loop-var #6602 and globals #6603 already landed).
