# SSA IR Design (Issue #8440)

*Last updated: 2026-07-08 (shared planned IR for register VM, Issue #9089)*

SubsetJuliaVM currently compiles Julia source through:

```text
Parser -> Lowering -> Core IR -> compiler -> stack VM bytecode
```

Issue #8440 introduces an SSA layer without forcing an immediate VM rewrite.
The first implementation slice keeps the external pipeline unchanged and adds a
temporary SSA-shaped Core IR pass. This lets the compiler prove and measure
small upstream-style optimizations before the durable SSA representation is
large enough to replace Core IR consumers.

## Target Shape

The long-term SSA layer should sit between Core IR and bytecode emission:

```text
Core IR -> SSA IR -> SSA optimization passes -> bytecode lowering -> VM
```

The durable representation should model:

- `SsaFunction`: function name, typed parameters, basic blocks, entry block.
- `SsaBlock`: block id, ordered statements, terminator, predecessor/successor
  edges.
- `SsaValue`: numbered SSA definitions plus constants and argument values.
- `PhiNode`: per-predecessor value joins at control-flow merge points.
- `EffectSummary`: local purity/throwing information reused from
  `compile/effects/` so SSA passes do not encode name-based shortcuts.

The first durable lowering target remains the existing stack VM bytecode, but
SSA lowering now first builds a backend-neutral `SharedFunctionPlan` (Issue
#9089). The stack backend emits `Instr` from that plan, and the register VM gate
lowers the same plan directly for functions that pass the SSA pipeline and the
current register subset.

## Durable Model (Issue #8550)

`subset_julia_vm_compile/src/compile/ssa_ir/` hosts the durable representation:

| File | Contents |
|------|----------|
| `model.rs` | `SsaFunction`, `SsaBlock`, `SsaStatement`/`SsaOp`, `SsaValue`, `PhiNode`, `Terminator` |
| `build.rs` | `build_function(&core::Function) -> Result<SsaFunction, SsaBuildError>` |
| `verify.rs` | `verify(&SsaFunction) -> Result<(), String>` |
| `dom.rs` | Reachability + Cooper–Harvey–Kennedy dominators, shared by `verify` and `opt` |
| `opt.rs` | Issue #8551 passes: `fold_constants`, `eliminate_unreachable_blocks`, `eliminate_dead_defs`, `cse_pure_calls`, `optimize(_with_effects)` |
| `scan.rs` | Syntactic read/write scanners backing the opaque-barrier model |
| `plan.rs` | Issue #9089 shared planned IR construction (`SharedFunctionPlan` roots, edge copies, and terminators) |
| `lower.rs` | Issue #8552 stack-bytecode lowering from the shared plan; ON by default (`SJULIA_SSA_PIPELINE=0` to disable); per-function fallback, `take_ssa_pipeline_stats` |

Where reality diverges from the original sketch:

- `SsaValue` ids are function-unique but **not positional** (unlike upstream's
  statement-index `SSAValue`s); each `SsaStatement` carries its own id, so
  block edits do not renumber values.
- `PhiNode` stores per-edge `Option<SsaValue>`s; `None` mirrors upstream
  `#undef` entries (variable maybe-undefined along that path). Phi `edges`
  must equal the block's predecessor list (ascending by block id).
- Operations this slice does not decompose are carried verbatim as
  `SsaOp::Opaque` (expressions) / `SsaOp::OpaqueStmt` (statements) with their
  resolved variable reads, so no construct is silently dropped — the same
  spirit as upstream IRCode keeping arbitrary `Expr` statements.
- `EffectSummary` reuse from `compile/effects/` landed with the pass slice
  (Issue #8551): the passes consume the body-derived `Effects` summaries
  directly instead of a dedicated SSA-side type.

### Core IR → SSA conversion

Core IR is structured (no arbitrary goto), so construction needs no iterated
dominance frontiers: at each merge point the incoming environments are known
and a phi is placed exactly where incoming values disagree (Braun et al. 2013
specialized to structured CFGs). Decomposed constructs: straight-line
statements, `x = e` / `x += e`, `if`/`else` (nested), `while` with
`break`/`continue`, ternary expressions, calls / builtins / unary / binary
operators, and explicit `return`. `while` headers pre-scan the body for
assigned variables and create one header phi each (sealed with the latch edges
after body conversion) — minimal at if-joins, **not pruned** at loop headers
(dead header phis are valid SSA; DCE is Issue #8551). Reads of names with no
local binding become `SsaOp::LoadGlobal`; names declared `global` route writes
through `SsaOp::StoreGlobal` and are never SSA-numbered. Unreachable code
(after `return`/`break`, or a join whose arms all return) is still converted
into predecessor-less blocks rather than dropped.

In debug builds `build_function` runs the verifier after every construction.
The verifier checks: dense block ids, terminator/succ/pred edge consistency,
entry has no predecessors, unique defs, phi-prefix placement, phi arity ==
predecessor arity, argument indices in range, and defs-dominate-uses via an
iterative Cooper–Harvey–Kennedy dominator tree (phi operands must dominate
their edge's predecessor; unreachable blocks are checked structurally but
skipped for dominance).

### Limitations of this slice

Conversion runs **behind unit tests only** — the compilation pipeline still
consumes Core IR. Lowering SSA back to bytecode is Issue #8552; passes are
Issue #8551. Known modeling limits, in decreasing order of importance:

- **`try`/`catch` is an opaque barrier**: the whole `Try` statement is one
  `SsaOp::OpaqueStmt` that reads every live local it mentions and rebinds
  every local it may assign via fresh `SsaOp::BarrierReload` defs ("spill
  everything live across it"). No exceptional CFG edges are modeled yet.
- `for`/`foreach` loops, destructuring/index/field/dict assignment, nested
  function definitions, and the test/timing macros take the same opaque
  barrier treatment (reads over-approximated, writes over-approximated but
  never under-approximated; construct-scoped binders like loop variables and
  `catch` variables are excluded from writes).
- `&&`/`||` are plain `SsaOp::Binary` ops; expression-position
  `return`/`break`/`continue` inside opaque payloads contribute reads/writes
  but no control edges.
- Rebinding of captured variables by closure *calls* is not modeled (the
  definition statement is a barrier; call sites are ordinary calls).
- `@goto`/`@label` are rejected (`SsaBuildError::UnstructuredControlFlow`).
- Keyword parameters are bound as trailing `SsaValue::Argument`s; their
  default expressions are not modeled. Implicit returns are modeled only for
  a trailing expression/assignment statement.

## Optimization Passes (Issue #8551)

`subset_julia_vm_compile/src/compile/ssa_ir/opt.rs` implements the first real passes
on the durable model. They run on `SsaFunction` behind **test-only** entry
points (`optimize` / `optimize_with_effects` plus the individual passes); the
compilation pipeline output is unchanged until bytecode lowering from SSA
lands (Issue #8552). In debug builds every pass re-runs the verifier after
mutating the function.

- **Constant folding/propagation** (`fold_constants`): operations whose
  operands are `SsaValue::Const` are evaluated through the shared
  `compile/const_prop` evaluators (`eval_const_unary` / `eval_const_binary`),
  so fold coverage and refusal cases (overflow, division by zero, unsupported
  operand types) stay identical to Core IR constant propagation. Trivial phis
  — all incoming values present and equal, ignoring loop-latch
  self-references — fold to their single value (Braun et al. trivial-phi
  rule), which subsumes the bridge's identical-branch-assignment fold on real
  `PhiNode`s. `Branch` terminators with constant `Bool` conditions rewrite to
  `Jump` (non-`Bool` constants keep the branch and its runtime TypeError;
  maybe-undef `None` phi edges block folding so undefined-variable error
  paths survive).
- **DCE** (`eliminate_unreachable_blocks`, `eliminate_dead_defs`): blocks
  left unreachable by branch folding are deleted with phi edges pruned and
  block ids densely renumbered; unused definitions are removed by liveness
  mark & sweep, which also collects dead def *cycles* — in particular the
  dead loop-header phis left by the unpruned `while` pre-scan (deferred from
  Issue #8550). Deletion requires `Effects::is_removable` (`:effect_free` +
  `:nothrow` + terminating), upstream's rule in
  `julia/Compiler/src/optimize.jl`.
- **Pure-call CSE** (`cse_pure_calls`): value numbering of calls in a
  depth-first walk of the dominator tree (shared with the verifier via
  `dom.rs`), so a duplicate merges only into an identical call that dominates
  it. Eligibility requires the callee summary to be `Effects::is_foldable`
  (`:consistent` + `:effect_free` + terminating + `inaccessiblememonly`);
  consistency excludes fresh-allocation callees (`Effects::allocating`,
  Issue #7176), and `:nothrow` is not required because with identical
  operands and no observable state the dominating call already exhibited any
  throw.

Call purity comes from the body-derived effect summaries of Issue #8441
(`infer_program_effects`), falling back to the curated
`infer_builtin_effects` name table (default arbitrary). Summaries are keyed
by **name**, so calls through a parameter (HOF pattern) or to a name rebound
by an opaque barrier (nested `function` definitions surface as
`BarrierReload` vars) are treated as arbitrary. Names rebound by **plain
assignment** (`f = sin; f(1)`) are handled by the `_scoped` pass entry
points (`optimize_scoped`, `eliminate_dead_defs_scoped`,
`cse_pure_calls_scoped`): the caller passes the source-level locally-bound
name set and those callee names are treated as arbitrary too — the gate site
does this (Issue #8799; the unscoped entry points keep the previous
behavior for callers without source access). `SsaOp::Builtin`, global
accesses, and opaque payloads are never assumed pure ("when in doubt,
keep").

Note on `infer_program_effects` (Issue #8441): its fixpoint originally
stopped after a **flat 100 worklist pops**, which on a real program (Base
alone is ~5k functions) left everything past the first 100 functions at the
`Effects::arbitrary()` seed — the summaries were effectively empty outside
unit tests. The valve now scales with program size (the merge is monotone,
so termination does not depend on it); this is what makes gate-site CSE of
user pure calls actually fire.

Note on per-method summaries (Issue #9205): the name-keyed map is the
conservative merge of **every** method sharing a name — the sound fallback when
a call site's dispatch is not statically resolved.
`infer_program_effects_per_method` additionally returns a `by_method` map keyed
by the inference engine's stable `MethodKey` (#8553), so a pure `f(::Int)` is
not conflated with an impure `f(::IO)` sibling — mirroring upstream, which
stores `ipo_effects` per `CodeInstance`, not per generic function.

Consumer wiring (Issue #9495): the DCE/CSE gates (`op_effects`) consult the
per-method summaries at **statically-resolved call sites** through
`effects::static_dispatch::StaticDispatchResolver`. At a bare call whose
argument types (constant literals or typed parameters) pin dispatch to a
*single, unambiguous* method of a **fully-visible multi-method generic**, the
gate uses that method's precise summary instead of the name-level merge — so a
pure `f(::Int)` shadowed by an impure `f(::Float64)` is still CSE'able/removable.
Soundness is paramount because this enables more aggressive transforms: the
resolver reuses the production typemap filter
(`dispatch_resolver::typemap_candidate_verdict`, #8548) and returns the precise
summary only when exactly one candidate is `Accept` and every other candidate is
`Reject`; a Base-defined or curated-builtin name (whose complete method set is
not visible), an imprecise argument type, ≥2 applicable methods, or any `Defer*`
verdict all fall back to the sound name-level merge. `SJULIA_EFFECTS_STATS=1`
logs how many foldable/removable methods the name merge hides
(`per_method_precision_stats`).

`Effects::noub` is tri-stated (`EffectBit::{AlwaysTrue,AlwaysFalse,Conditional}`,
Issue #9496), mirroring upstream `NOUB_IF_NOINBOUNDS`
(`julia/Compiler/src/effects.jl:169-176`): `array_getindex`/`array_setindex`/
`Expr::Index` classify `Conditional` instead of `AlwaysFalse`, since sjulia's
default indexing bytecode always bounds-checks (UB is only reachable through
the compiler's own statically-proven-in-bounds `IndexLoadInbounds`/
`IndexStoreInbounds` fast path). `is_foldable()` accepts `Conditional` here,
matching upstream's `is_noub(effects) || is_noub_if_noinbounds(effects)`. The
merge for `noub` uses the dedicated AF-absorbing `EffectBit::merge_af_absorbing`
(not the symmetric `EffectBit::merge` that `consistent`/`effect_free` use), so
an `AlwaysFalse` (proven-UB-possible) branch can never be diluted to
`Conditional` by merging with a proven-safe branch. Measured over the whole
Base corpus (1313 name-level summaries): zero `is_foldable()` regressions,
zero newly-foldable, 14 summaries reclassified to `Conditional` — see
`docs/vm/STATUS.md` 2026-07-10 for the full write-up. `nothrow` was
investigated as a companion entry point but is NOT tri-stated: upstream keeps
it a plain `Bool` (no `NOUB_IF_NOINBOUNDS`-style override exists for it), and
sjulia already models its conditional refinement at the right layer —
per-call-site, argument-type-sensitive discharge in
`compile::abstract_interp::engine` (e.g. `sqrt`/`log` domain-narrowing).

## Bytecode Lowering (Issue #8552)

`subset_julia_vm_compile/src/compile/ssa_ir/lower.rs` is the first SSA backend: it
lowers an optimized `SsaFunction` to the existing stack-VM `Instr` stream.
The SSA pipeline is **ON by default** (Issue #8832 default flip); set
`SJULIA_SSA_PIPELINE=0` to disable and force the legacy Core-IR path. The
env var is checked once per program compile in `compile_functions`.
**User-scope** function bodies go

```text
Core IR → build_function → ssa_ir::opt (fold/DCE/CSE fixpoint) → lower → slotize/peephole
```

Inside `lower`, `plan::plan_function` first produces a `SharedFunctionPlan`.
The stack backend emits bytecode from that plan immediately, and the compiled
`FunctionInfo` keeps a runtime-only copy (`shared_plan`, skipped by cache
serialization) so the register VM gate can lower from the same planned IR
without translating the emitted stack bytecode (Issue #9089).

The passes run through `optimize_scoped` with the body-derived
`infer_program_effects` summaries (Issue #8441), computed lazily once per
gated program compile at the gate site (~6 ms on a Base-cached compile), and
with the function's locally-bound name set so plain-assignment rebinds
(`abs = println; abs(-5)`) are never attributed the summary of an unrelated
global or builtin of the same name (Issue #8799 — before this, DCE could
delete the very call whose presence forces the legacy fallback, silently
dropping its side effect).

and everything else — every Base/prelude function (so the Base cache is
always legacy-built), plus any function the lowering cannot prove
equivalent — falls back to `CoreCompiler::compile_function_body` **per
function**, before any bytecode is emitted. `SJULIA_SSA_PIPELINE_LOG=1`
logs the per-function decision with the fallback reason;
`ssa_ir::take_ssa_pipeline_stats()` exposes lowered/fallback counters to
tests.

### Design

The plan is backend-neutral, while the stack emitter still reuses the legacy
emitters for instruction selection. That split is what makes round-trip parity
tractable while giving the register backend a non-stack source:

- **Scheduling**: blocks are emitted in id order (construction order, which
  the passes preserve through dense renumbering); terminators become
  fallthroughs or `Jump`/`JumpIfZero` with two-phase target patching.
  `Branch` conditions go through the legacy
  `compile_condition_false_jumps`, inheriting short-circuit emission and
  Bool-context TypeError semantics.
- **Value materialization**: definitions with stack-shaped lifetimes (single
  use, same block, operand chains matched right-to-left against adjacent
  statements so evaluation order is preserved exactly) are rebuilt into
  nested Core IR expression trees and compiled with `compile_expr` — for
  straight-line code the reconstructed tree is the original expression, so
  the bytecode matches the legacy output. Everything else **spills** to a
  synthetic local (`#ssaN`) emitted as a `Stmt::Assign`; `vm/slot.rs`
  slotization assigns the frame slots. The reconstructed statement stream is
  pre-scanned with the same `collect_local_types_with_mixed_tracking` the
  legacy path runs, so spill slots get identical widening/mixed-type
  treatment.
- **Phi nodes become slot writes on incoming edges**: copies are appended to
  `Jump` predecessors; critical `Branch` edges get a trampoline block
  (patch the false-jumps to the copies, then jump on). Interfering parallel
  copies (loop-carried swaps) are staged through `#ssatmpN` temporaries in
  two rounds; values flowing on conditional or multi-copy edges are always
  spilled so their evaluation stays unconditional and in original order.
- **Phi-copy coalescing** (Issue #8440): when the value flowing into a phi
  along a `Jump` edge is produced in the jumping block itself and dies at
  the copy, the definition writes the phi's slot directly and the edge copy
  is elided. Requirements: `Jump` predecessor (single successor), the copy
  is the definition's only use, nothing after the definition reads the
  phi's previous value (later statement operands, terminator operands,
  other copies on the same edge), and the edge's parallel copies do not
  interfere (interfering edges keep the temp staging untouched). This
  removes the spill-store + slot-to-slot copy per loop-carried variable and
  restores the legacy self-update store shape (`k = k + 1` on one slot), so
  the peephole loop fusions (`AddConstI64Slot`,
  `AddConstI64SlotAndJumpIfLe`, `JumpIfGtI64Slots`) apply — the gated
  three-variable `calc_pi` loop now emits bytecode identical to the legacy
  path.
- **Branch-type propagation** (Issue #9085): before emitting each block,
  `compute_block_narrowing_info` checks whether the block is uniquely
  dominated by one arm of a `Branch` (exactly one predecessor, and that
  predecessor's terminator is the branch). If so, the branch condition's
  `isa`/`typeof(x) === T`/`=== nothing` guard facts are overlaid onto
  `compiler.locals` via the same `apply_then_narrowings` /
  `apply_else_narrowings` the legacy `compile_if_stmt` uses (`narrowing.rs`,
  Issue #5077), and restored after the block's roots and terminator are
  emitted. This folds dominated redundant `isa` re-checks to
  `PushBool(true)` and specializes guarded arithmetic (`AddI64` instead of
  `DynamicAdd`), closing the ~1.7x `union_isa_elision` regression measured
  after the default flip. Join blocks (≥2 predecessors) and jump-reached
  blocks get no narrowing, which is exactly the legacy scoping (facts never
  leak past the guarded region).
- **Returns** reproduce the two legacy tail emissions: bodies ending in an
  explicit `return` use the explicit instruction choice
  (`should_return_as_expected_type`), bodies ending in a trailing expression
  use the implicit one (I64↔F64 `emit_type_conversion` toward the declared
  return type).

### Per-function fallback conditions

`SsaBuildError` (`@goto`/`@label`), any opaque barrier op
(`for`/`try`/mutating statements/nested functions/string interpolation —
the build.rs limitation list), closures with captured variables, keyword
parameters, maybe-undefined phis (legacy `UndefVarError` carries the source
variable name, which SSA erased), calls through locally rebound names
(`f = sin; f(x)` — the name-keyed limitation shared with the effects
machinery), module-valued global reads (`S = Statistics` alias tracking is
name-keyed), `&&`/`||` in **statement position** (result discarded —
`x <= 0 && throw(Err)` at the statement level: the SSA builder evaluates
both operands unconditionally, so DCE can remove the condition guard while
keeping the side-effectful right operand, silently breaking the short-circuit
guard; falls back per function — Issue #8832), `&&`/`||` used as an `if`/`while`
condition remain eligible (their result feeds the `Branch` terminator and
the legacy `compile_condition_false_jumps` restores short-circuit semantics
during value materialization), bodies with
an implicit tail mixed with explicit returns under an I64/F64 return type,
and unsupported tail statement kinds (a statement that is not a `Return` or
trailing expression and cannot be proven to always return — e.g. `while` at
the end of the body; `if`/`else` where **both** branches always return is
now eligible as `TailMode::Explicit`, Issue #8832).

**Runtime-specialized functions that store locals** (Issue #8440): functions
in `spec_func_mapping` (untyped params) get `CallSpecialize` call sites, and
the VM's `install_specialized_body` slotizes the runtime-specialized
bytecode against the **generic body's slot-name table**
(`FunctionInfo::slot_names`). An SSA-lowered body publishes `#ssaN` slot
names instead of the source locals the specialized bytecode stores to, so
every specialized access would degrade to name-based instructions (measured
5× on the untyped `calc_pi` loop: 0.22 s legacy vs 1.2 s lowered). Bodies
without local stores (e.g. `fib`) publish a params-only table on both paths
and stay eligible. Lifting this requires either source-named slots in the
lowering (a de-SSA/variable-web step with real interference analysis) or a
specializer that owns its slot table.

### Round-trip parity

`subset_julia_vm/tests/ssa_pipeline_parity_8552_tests.rs` runs targeted
shapes (straight-line, if/else phi, while-loop phi + break/continue,
ternary, short-circuit, implicit-return conversion, recursion, globals,
loop-carried coalescing, loop-swap interference, locally-rebound call side
effects, branch-dominated pure-call CSE, `&&`/`||`-in-statement-position
fallback regression — Issue #8832) and the full `const_prop`, `closures`,
`dispatch`, `control_flow`, `function`, and `functions` fixture categories
through both paths, diffing final value, printed output, and error strings,
and asserting the gated run actually lowers functions. Two bytecode-shape
tests additionally pin the Issue #8440 optimizations: the gated `branchy`
body must contain exactly one call to the pure callee (legacy keeps three),
and the gated three-variable loop body must not exceed the legacy instruction
count.

### Measured (2026-07-02, release build, single machine, medians of 5)

First slice (PR #8605): `calc_pi` 2M-loop 0.18 s legacy → 0.60 s gate-on
(loop-phi spill), `fib` parity, `optim_bfgs_rosenbrock.jl` no compile-time
regression (0 lowered / 9 fallbacks). After the Issue #8440 coalescing +
effects slice (CLI wall times; background agents were compiling during some
runs — the Criterion rows below were taken at 1-min load < 8):

| Workload | Gate off | Gate on | Note |
|----------|---------:|--------:|------|
| `calc_pi(n::Int64)` `while` loop, 2M iterations (CLI) | 0.19 s | 0.19 s | lowered via SSA; **bytecode identical to legacy** (coalescing restores the one-store-per-carried-variable shape and the `AddConstI64SlotAndJumpIfLe` / `JumpIfGtI64Slots` fusions); was 0.60 s gate-on before coalescing |
| `calc_pi(n)` untyped, 2M iterations (CLI) | 0.20 s | 0.21 s | falls back (runtime-specialized + local stores); before the fallback the lowered slot renaming degraded the *specialized* body to name-based instructions (1.2 s) |
| `fib(25)` typed recursion (CLI) | 0.13 s | 0.14 s | lowered; the reconstructed body is one instruction *shorter* than legacy (no jump-to-fallthrough after the `if`) |
| `fib(25)` untyped recursion (CLI) | ≈4.1 s | ≈4.1 s | lowered (no local stores → params-only slot table on both paths); dominated by the pre-existing per-call specialization dispatch cost, gate-independent |
| Criterion `ssa_pipeline_gate/calc_pi_loop_carried` (`Vm::run()` only, 1M iterations) | 38.4 ms | 38.8 ms | parity within 1.1% — flip criterion 3's spill gap is closed |
| Criterion `ssa_pipeline_gate/cse_branch_dominated` (`Vm::run()` only, 200k-loop over a branch with a dominated repeated pure call) | 126.9 ms | **91.3 ms** | **gate-on 1.39× faster (−28%)**: SSA pure-call CSE emits 1 `CallResolved` per `branchy` invocation vs. 3 on the legacy path (shape-pinned by `ssa_cse_reduces_branch_arm_calls_in_bytecode_issue_8440`) — the Issue #8440 "measurable improvement on a benchmark" acceptance criterion |

Compile-time cost of the gate-site effects pass: ~6 ms on a Base-cached
compile (lazy, once per gated compile; gate-off pays nothing).

### Go/no-go criteria for flipping the default

1. Parity: the full fixture suite (not just the three covered categories)
   green with the gate on, including error-message/span parity. **Met
   (Issue #8832)**: the `&&`/`||`-in-statement-position correctness bug is
   fixed; the parity test covers `const_prop`, `closures`, `dispatch`,
   `control_flow`, `function`, and `functions` categories, all green. The
   default is now ON; `SJULIA_SSA_PIPELINE=0` forces the legacy path.
2. Coverage: the opaque-barrier constructs (`for`, `try`, mutation,
   closures) decomposed or natively lowered so the fallback rate on real
   workloads (bundled packages) is a small tail rather than the majority.
   **Open**; the Issue #8440 slice added one more fallback class
   (runtime-specialized bodies with local stores) that needs source-named
   slots to lift. Correctness is maintained by the per-function fallback.
3. Performance: no cold compile-time regression, Base cache load unchanged,
   and gate-on VM runtime at parity or better on `benches/`. **Met for the
   lowered set**: phi-copy coalescing closed the loop-spill gap (gate-on
   loop bytecode is legacy-identical), the specializer-contract fallback
   removed the untyped-loop regression, and
   `benches/ssa_pipeline_gate_benchmark.rs` records one workload where the
   SSA path beats legacy (branch-dominated CSE). Remains open for the
   constructs that currently fall back (criterion 2).
4. Effects: `optimize_with_effects` wired to `infer_program_effects`
   summaries at the gate site, with call-name misattribution resolved.
   **Met** (Issues #8799 + the `propagate_effects` valve fix): the gate runs
   `optimize_scoped` with body-derived summaries and the locally-bound name
   set; the residual name-keyed conservatism (multi-method Base names like
   `sqrt` / n-ary `*` merge to arbitrary) is inherent to name-keyed
   summaries and documented above. Per-method summaries now exist
   (`by_method`, Issue #9205) and the opt passes consume them at
   statically-resolved call sites through `StaticDispatchResolver`
   (Issue #9495), recovering the multi-method precision the merge hid for
   fully-visible user generics.

## Temporary Bridge (first slice — retired Issue #8832)

`subset_julia_vm_compile/src/compile/ssa_ir/bridge.rs` was a temporary bridge, retired
as part of the Issue #8832 default flip. It recognized a Core IR pattern that
corresponds to a trivial Phi:

```julia
if flag
    x = 41
else
    x = 41
end
```

The pass rewrote this to an empty conditional that still evaluated and checked
the condition, followed by one joined assignment:

```julia
if flag
else
end
x = 41
```

This removes duplicate branch stores after slotization. The regression test for
`same_branch_phi_8440` measures the concrete bytecode effect: `x` now has only
the initializer plus one joined `StoreSlotI64`, instead of one initializer plus
one store in each branch.

The pass is deliberately limited to identical literal assignments. Literal
payload equality ignores source spans, matching SSA value semantics while
avoiding broader expression motion until the effect model is wired into SSA.

## Compiler Integration (bridge — retired)

The bridge ran at the end of `compile::ir_opt::IrOptimizer::optimize_block`.
That call was removed in Issue #8832 (default flip); the identical-branch fold
is now subsumed by SSA `fold_constants`'s trivial-Phi rule on real `PhiNode`s
for all SSA-lowered bodies, and the legacy path no longer needs it.

## Remaining Work

- ~~Define durable `SsaFunction`, `SsaBlock`, `SsaValue`, and `PhiNode`
  structs.~~ Done (Issue #8550).
- ~~Convert Core IR blocks into an explicit CFG with predecessor/successor
  edges.~~ Done for structured control flow (Issue #8550); see the limitation
  list above for the constructs still treated as opaque barriers.
- ~~Move the literal Phi fold from the temporary bridge onto real Phi nodes.~~
  Done (Issue #8551): `fold_constants` folds trivial/constant phis.
- ~~Remove the Core-IR-level bridge call from `ir_opt`.~~ Done (Issue #8832):
  `bridge.rs` retired; `ir_opt` no longer calls `fold_identical_branch_assignments`.
- ~~Add constant Phi folding and local DCE on SSA values, including the dead
  loop-header phis left by the unpruned pre-scan.~~ Done (Issue #8551), plus
  pure-call CSE; all behind test-only entry points.
- ~~Lower SSA back to existing stack bytecode without regressing cold compile
  time or Base cache load time (Issue #8552).~~ Done (Issue #8832): SSA
  pipeline is now the default; set `SJULIA_SSA_PIPELINE=0` to use legacy path.
- ~~Flip the SSA pipeline default.~~ Done (Issue #8832).
- ~~Close the loop-phi spill gap (typed spill slots + phi-copy
  coalescing).~~ Done (Issue #8440): spill slots were already typed by the
  shared pre-scan; phi-copy coalescing removed the double stores and
  restored the peephole loop fusions — gated loop bytecode is
  legacy-identical.
- ~~Extract the backend-neutral planned IR consumed by stack and register
  lowering.~~ Done (Issue #9089): `SharedFunctionPlan` lives in the bytecode
  crate, stack SSA lowering emits from it, and the register VM gate consumes
  `FunctionInfo.shared_plan` instead of translating stack bytecode.
- ~~Wire `optimize_with_effects` at the gate site, resolve call-name
  misattribution (Issue #8799), and measure at least one `benches/`
  Criterion case where the gate wins.~~ Done (Issue #8440):
  `optimize_scoped` + `infer_program_effects` (with the fixpoint valve
  scaled to program size) + `benches/ssa_pipeline_gate_benchmark.rs`.
- Lift the runtime-specialization fallback: give lowered bodies source-named
  slots (a de-SSA/variable-web step with interference analysis) or teach
  `install_specialized_body` to own its slot table, so untyped-param
  functions with local stores can be lowered without degrading the
  specialized body.
- Decompose the opaque barrier constructs (`for`, `try`/`catch` with real
  exceptional edges, destructuring) once passes/lowering need them.
- Add VM-only and CLI measurements once SSA covers hot-loop patterns beyond this
  first bytecode-shape slice.
