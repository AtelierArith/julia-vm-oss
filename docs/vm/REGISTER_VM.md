# Register VM Decision Record (Issue #8448)

*Last updated: 2026-07-10 (#8446 / #8448 / #8558 / #8559 / #9089 / #8561 / #8562
are closed; #9904 / #9906 P6 measurements are recorded below; default-switch
follow-up remains gated on the normal release decision path)*

## Decision

SubsetJuliaVM should pursue a register VM as the preferred long-term
interpreter shape for iOS and WebAssembly, while keeping the current stack VM as
the default execution engine until a side-by-side prototype proves parity and
target-specific measurements.

The decision is architectural, not a switch-over. The next implementation slice
must add a gated prototype that can run the same fixture through:

- host `sjulia`
- iOS Simulator
- WebAssembly

The prototype must report bytecode size, dispatch count, VM-only execution
time, and frame/register memory before the project changes the default VM.

## Context

Issue #8446 improved the existing stack VM hot path first. That remains the
right near-term order: stack bytecode is production-owned, broadly tested, and
already integrates with the C ABI, WebAssembly bindings, Base cache, panic-free
runtime policy, and fixture suite.

Issue #8440 adds an SSA-shaped optimization path. That path gives the register
VM a natural lowering source because Phi nodes, typed values, and explicit
control-flow edges can map to virtual registers without reconstructing stack
effects from final bytecode.

Issue #8448 asks whether register VM design is viable on iOS and WebAssembly.
The no-JIT constraint does not rule it out. A portable register interpreter is
still ordinary ahead-of-time Rust code; it does not require executable memory,
runtime code generation, or platform-specific direct threading.

## Target Shape

The target pipeline should be:

```text
Core IR -> SSA IR -> SSA optimization passes
                   -> stack bytecode lowering -> stack VM
                   -> register bytecode lowering -> register VM
```

During migration, stack and register bytecode should be generated from the same
typed SSA representation where possible. This keeps semantic checks, source
spans, constant propagation, and effect decisions in the compiler instead of
duplicating them in two VM backends.

Register bytecode should make these properties explicit:

- operand registers for inputs and outputs
- statically computed frame/register count per function
- branch targets over basic blocks, not implicit stack depth
- call instructions that name argument register ranges and destination
  registers
- typed fast instructions where the compiler has a stable `ValueType`
- dynamic fallback instructions for `Any`, method dispatch, and boxed values

## Dispatch Strategy By Target

### Host and iOS

Start with a portable Rust `match` loop over register instructions. It is the
lowest-risk shape for iOS because it stays within ordinary static code and keeps
panic-free error paths inspectable.

Function-pointer tables or direct-threaded dispatch may be evaluated later only
if profiling shows a clear win and the implementation remains acceptable for
Rust, iOS, and sanitizer/debug builds. The first prototype should not depend on
computed goto or writable/executable memory. (Evaluated for the *stack* VM in
the Issue #8562 experiment below: the fn-pointer table lost to the `match` on
every target — see "Handler-Table Dispatch Experiment".)

### WebAssembly

Assume the portable `match` loop is the baseline. Wasm engines can lower dense
opcode switches into `br_table` or equivalent switch machinery, but SubsetJuliaVM
should not rely on direct threading or host-specific assembly tricks.

The Wasm prototype must measure code size and dispatch count because reducing
stack shuffling can be more valuable than micro-optimizing the dispatch loop.

### Frame Layout

Each compiled register function should carry a fixed register-frame length.
Registers initially hold `Value` so the prototype can share existing runtime
operations and error handling. Typed register storage can be added later for
hot scalar loops once measurements show which values are worth unboxing.

Large objects, arrays, structs, closures, and heap-owned Julia values remain
heap references inside `Value`; the register frame owns references, not object
payloads. This avoids changing GC/ownership assumptions while the backend is
still side-by-side with the stack VM.

## Prototype Gate

The first implementation slice after this decision record should be gated behind
an explicit feature flag or internal developer option and should not affect
default fixture execution.

Minimum proof:

- compile one straight-line typed fixture to register bytecode
- execute it on host through the register VM path
- execute the same fixture through iOS Simulator FFI
- execute the same fixture through WebAssembly bindings
- compare output with upstream Julia and the current stack VM

Minimum measurements:

- stack bytecode bytes vs. register bytecode bytes
- stack VM dispatch count vs. register VM dispatch count
- VM-only execution time with precompiled program reuse
- register frame count and estimated frame memory
- Wasm artifact size impact for the gated prototype

Cold CLI timing is not sufficient for this decision because parser, lowering,
Base cache, and startup costs can hide the interpreter effect.

## Migration Rules

- Keep stack VM bytecode as the default until register VM parity and target
  measurements are available.
- Do not add package-name, module-name, or type-name shortcuts while building
  the register backend.
- Reuse the shared runtime type-system facade instead of adding fresh
  compile-to-VM ownership leaks.
- Keep source spans and structured `VmError` behavior equivalent between the
  stack and register paths.
- Treat mismatches as compatibility bugs, not benchmark noise.
- Remove register-VM-only tests or options before defaulting if they cannot run
  on host, iOS Simulator, and WebAssembly.

## Prototype Status (Issues #8558 / #9089, 2026-07-08)

`subset_julia_vm::register_vm` now lowers SSA-planned compiled *function
bodies* (loops, branches, typed slots, `Float64` arithmetic, comparisons,
calls) from the backend-neutral `SharedFunctionPlan` into an in-memory register
form and interprets them with a portable `match` loop. Registers are allocated
by the register backend; local slots keep separate storage and boxed/heap-owned
values stay by-reference inside `Value`. The older stack-bytecode translator is
kept for standalone metrics and regression tests, but the opt-in VM gate no
longer translates stack bytecode.

### Gate

`SJULIA_REGISTER_VM=1` (read at `Vm` construction) routes eligible direct
calls (`Call` / `CallInbounds` / `CallResolved`) through the register VM;
ineligible or untranslatable functions stay on the stack VM, memoized per
function index. Since Issue #9089, a function must carry
`FunctionInfo.shared_plan` to be eligible; Base/prelude-cache methods and
legacy Core-IR fallback bodies (for example current opaque-barrier `for`
loops) remain on the stack VM instead of going through the old stack-bytecode
translator. `SJULIA_REGISTER_VM_LOG=1` logs which path each function took
(translated + metrics, or the named fallback reason). The gate is off by
default and costs one `Option` check on the direct-call path.
Parity harness: `subset_julia_vm/tests/register_vm_parity_8558_tests.rs` runs
`fib`-class recursion and `calc_pi`-class loops (while + for forms) with the
gate off and on and diffs the printed output (also pinned against upstream
Julia).

### Call boundary

Calls inside a register-executed body now have a P1 register-native path
(Issue #9904 / #9906): `RegisterInstr::CallStack` first asks the
`RegisterVmHost` to prepare a register callee frame. If the callee has a
translatable `SharedFunctionPlan` and matching parameter-slot layout, the
register interpreter pushes an explicit frame (`program + slots + registers +
pc`) onto its own heap-backed frame stack and resumes the caller when the callee
returns. This removes host-Rust-stack recursion for recursive register calls;
`countdown(500)` executes all 501 `countdown` invocations on register frames.

The fallback boundary remains explicit. Untranslatable callees still trampoline
back into the stack VM through `RegisterVmHost::call_function`: the host pushes
a regular stack frame and drives `run_until_frame_return` until the callee
returns, reusing the `eval_dispatch_call` nested-dispatch discipline
(ancestor-handler floor, error-path unwinding — Issues #4976 / #5972 / #7687).
`CallIntrinsic` remains a stack-host operand operation. The
`MAX_REGISTER_VM_NESTING = 64` cap now protects only re-entrant register
executions reached through stack-VM fallback trampolines; native
register-to-register calls use the explicit register frame stack instead. The
native frame stack also mirrors the stack VM call-depth guard and returns a
catchable `StackOverflowError` instead of recursing until host OOM.

### Shared-plan coverage

Translated by the gate (`RegisterProgram::from_shared_plan_with_context`):

- Roots and phi copies: spilled assignments and discarded expressions.
- Literals / slots: `Int64`, `Float64`, `Bool`, `Nothing`, source slots, and
  synthetic `#ssaN` slots.
- Arithmetic: `Int64` add/sub/mul/rem, `Float64` add/sub/mul/div, unary
  negation, Bool `!`, and typed comparisons.
- Control flow: block-order lowering, unconditional jumps, conditional
  branches, and edge-copy trampolines.
- Calls: single statically-resolved function candidate via
  `RegisterInstr::CallStack`; translatable callees run as native register
  frames, with stack-VM fallback for ineligible callees.
- Returns: Any/Nothing register returns.
- Explicit numeric type-constructor calls (Issue #9803): `Expr::Convert`, a
  structural node the shared plan builder
  (`compile::ssa_ir::plan::numeric_convert_target`) rewrites a bare
  `Int64(x)`/`Float64(x)` call into at plan-build time (where resolving a
  call target by name is already the established mechanism). The rewrite is
  guarded by a per-function `NumericConvertGate` computed in
  `lower.rs::numeric_convert_gate` from the same evidence the stack
  compiler's `compile_generic_dispatch_call` consults: it fires only when
  the name has no reachable method table (bare, module-owned, or
  `Base.`-qualified) and no parameter / where-clause type param shadows it —
  a program defining `Float64(::MyIrrational{:tau})` (dispatch fixture,
  Issue #633) keeps the plain `Expr::Call`, whose stack lowering performs
  full user-method dispatch and whose register lowering falls back. The
  register backend dispatches on the `NumericConvertTarget` enum
  discriminant — never
  a string compare on the callee name — and only lowers the subset proven
  identical to the stack builtin: `Float64(x)` with a statically I64 operand
  (`RegisterInstr::I64ToF64`, documented "Stack `ToF64` parity" — always
  rounds, never raises) and the two identity cases (`Float64(::F64)`,
  `Int64(::I64)`). `Int64(x)` with a statically F64 operand is deliberately
  left on the stack path: the stack builtin
  (`CallBuiltin(BuiltinId::Int64, 1)`) is a *checked* constructor that raises
  `InexactError` on a non-integral float (verified against both `julia` and
  `sjulia`), while `RegisterInstr::F64ToI64` truncates — lowering it natively
  would silently diverge from that semantics. Any operand whose numeric kind
  is not statically known (`Any`, `Bool`, other) also falls back. Stack
  lowering of `Expr::Convert` is unchanged: it compiles to the exact same
  `CallBuiltin(BuiltinId::Int64/Float64, 1)` sequence
  `compile_builtin_types`'s `"Int64"`/`"Float64"` arms always emitted, so the
  existing peephole fusions (`LoadSlotI64ToF64`, `AddF64I64Slots`, ...) still
  apply and the marker-scan gate above still requires those fusions to
  actually remove the `CallBuiltin` before the whole function is
  register-eligible.

Unsupported expression forms fail shared-plan register lowering explicitly and
leave the whole function on the stack VM. The gate also keeps whole functions on
the stack VM when their final stack bytecode still contains unsupported semantic
markers such as dynamic dispatch calls, builtin/specialized calls, dynamic
numeric conversions, or name-based global load/store instructions; this avoids
running shared-plan register code in functions whose remaining stack lowering
still controls return boxing or global semantics.

### Standalone stack-instruction coverage

Translated by the retained test/metrics path (`RegisterProgram::from_stack_function`):

- Constants: `PushI64`, `PushF64`, `PushBool`, `PushNothing`
- Slots: `LoadSlotI64`, `StoreSlotI64`, `LoadSlotF64`, `StoreSlotF64`,
  `LoadSlotI64ToF64`, `LoadSlot`, `StoreSlot`
- I64 arithmetic: `AddI64`, `SubI64`, `MulI64`, `ModI64`, `IncI64`, `NegI64`,
  `AddConstI64Slot`, `LoadAddConstI64Slot`
- F64 arithmetic: `AddF64`, `SubF64`, `MulF64`, `DivF64`, `PowF64`, `NegF64`,
  `Load{Add,Sub,Mul,Div}F64Slot`, `LoadSquareF64Slot`
- Comparisons: `{Lt,Le,Gt,Ge,Eq,Ne}I64`, `{Lt,Le,Gt,Ge,Eq,Ne}F64`
- Conversions / shuffles: `ToF64`, `ToI64`, `BoolToI64`, `I64ToBool`,
  `NotBool`, `Dup`, `DupI64`, `DupF64`, `Pop`
- Control flow: `Jump`, `JumpIfZero`, `JumpIf{Eq,Ne,Lt,Gt,Le,Ge}I64`,
  `JumpIf{Eq,Ne}F64`, `JumpIfNot{Lt,Gt,Le,Ge}F64`, `JumpIfGtI64Slots`,
  `AddConstI64SlotAndJumpIfLe`
- Calls (stack-VM trampoline): `Call`, `CallInbounds`, `CallResolved`,
  `CallResolvedI64Slots`, `CallInboundsI64Slots`, `CallIntrinsic`
- Returns: `ReturnI64`, `ReturnF64`, `ReturnAny`, `ReturnNothing`

Every other stack instruction fails translation with an `Err` naming the
instruction (no silent per-instruction fallback). This path is no longer used
by the `SJULIA_REGISTER_VM=1` gate. Notable exclusions in this slice:
`CallBuiltin` and the dynamic-dispatch call family, name-based
`Load*/Store*(String)` locals, Bool/Str/Char/collection typed slots,
exception handlers (`PushHandler` …), `SelectI64/F64`, the F64 math opcodes
(`SqrtF64`, `FloorF64`, …), the reversed-operand `LoadAddI64Slot` fusion
family, and `IncVarI64Slot`/`DecVarI64Slot`.

### Measured metrics (feeds Issue #8559)

Static translation metrics from
`register_vm_metrics_for_covered_fixtures_issue_8558` (host, release
`--nocapture`, 2026-07-08):

| Function body | Stack instrs | Register instrs | Register bytes | Registers | Slots |
|---------------|--------------|-----------------|----------------|-----------|-------|
| `fib(n)` (recursive) | 11 | 13 | 520 | 2 | 1 |
| `calc_pi(n)` (while loop) | 28 | 28 | 1,120 | 4 | 4 |
| `calc_pi_for(n)` (for loop) | 28 | 32 | 1,280 | 5 | 5 |

Dynamic register execution totals from the parity harness
(`register_vm_executed_calls` / `register_vm_dispatch_total`, same run):

| Fixture run | Register calls | Stack-fallback calls | Register dispatches |
|-------------|----------------|----------------------|---------------------|
| `fib(20)` | 10,945 | 23 | 131,345 |
| `calc_pi(100000)` (while) | 1 | 23 | 2,300,016 |
| `calc_pi_for(100000)` (for) | 0 | 24 | 0 |
| `countdown(500)` (native register recursion) | 501 | 22 | 5,507 |

Notes: `fib` alternates engines by call-tree level (register bodies
trampoline recursive calls into stack frames, whose calls gate back to the
register VM), so 10,945 of the 21,891 total invocations run on registers.
`calc_pi_for` is output-parity checked but currently stack-only because the SSA
pipeline treats `for` as an opaque barrier, so the function has no shared plan
for the #9089 gate.
`countdown(500)` now shows the P1 native-frame path: all 501 invocations run on
the register interpreter's explicit frame stack. The remaining fallback calls in
that run are non-countdown startup/host calls. VM-only execution *time* and the
iOS/Wasm columns remain for the Issue #8559 measurement slice.

## Measurements (Issue #8559, 2026-07-02)

Cross-target register VM vs stack VM matrix, measured 2026-07-02 at
commit `30f98b862` (the #8559 measurement-harness commit on
`feat/8559-regvm-measurements`), Apple Silicon macOS host.

### Method

- **Engines.** Stack VM = the production engine exactly as shipped,
  *including* its executable-block fast path and call specialization.
  Register VM = the #8558 gate routing eligible direct calls to the
  prototype (calls inside register bodies still trampoline into stack
  frames).
- **Benchmarks** (the #8448 target list; output pinned against upstream
  Julia and byte-identical on both engines on every target):
  `fib(25)` recursion, `calc_pi(1_000_000)` while loop,
  `lorenz_accum(1_000_000)` attractor-style Float64 loop.
- **Harnesses.** Host and iOS Simulator run the same binary,
  `register_vm_bench_8559` (`--release`; iOS via
  `cargo build --target aarch64-apple-ios-sim` + `xcrun simctl spawn` on
  iPad (A16), iOS 26.1). Wasm runs `subset_julia_vm_web::RegisterVmBench`
  under Node v24.14.1 via `scripts/register_vm_wasm_bench_8559.mjs`
  (`wasm-pack build --target nodejs --profile web-release`).
- **Wall time** = median of 7 `Vm::run()` executions of a precompiled
  program (parse/lower/compile excluded; fresh `Vm` per run; counters
  disabled). **Counters** come from one separate instrumented run per
  engine (`SJULIA_STACK_VM_METRICS` / `RegisterGateState`); they are
  deterministic and load-independent, and came out **identical on host,
  iOS Simulator, and Wasm**.
- **Noise caveat.** The host was NOT machine-quiet on measurement day
  (concurrent builds; load average ~20). The single-threaded harness
  medians proved robust — re-runs under that load reproduced every
  number within 3%, and within-engine sample spreads stayed < 2%
  (lorenz iOS worst case ~7% outlier) against deltas of 25–640% — but
  the Criterion companion
  (`cargo bench -p subset_julia_vm --bench register_vm_gate_benchmark`)
  caught load spikes and its wall numbers were discarded; re-run it
  machine-quiet before quoting Criterion output. Counter columns are
  deterministic and unaffected by load.

### Static metrics (per function body, target-independent)

Instruction strides: stack `Instr` = 72 B, `RegisterInstr` = 40 B,
`Value` = 64 B.

| Body | Stack instrs | Stack bytes | Register instrs | Register bytes | Registers | Slots |
|------|--------------|-------------|-----------------|----------------|-----------|-------|
| `fib` | 12 | 864 | 13 | 520 | 2 | 1 |
| `calc_pi` | 28 | 2,016 | 28 | 1,120 | 4 | 4 |
| `lorenz_accum` | 54 | 3,888 | 54 | 2,160 | 2 | 10 |

Register bytecode is ~46% smaller by bytes at near-identical instruction
counts (the 40 B in-memory stride is not a compact encoding yet — both
columns would shrink under a serialized format).

**Per-frame memory.** Register VM allocates
`(registers + slots) × 64 B` fresh per call (fib 192 B, calc_pi 512 B,
lorenz 768 B). The stack VM `Frame` struct is 256 B plus its
`locals_slots` allocation plus a share of the *shared* operand stack —
dynamic high-water marks below show that share is small (≤ 14 slots for
the whole fib(25) run across 26 live frames), so per-frame footprints are
comparable; neither engine dominates.

### Dynamic counters (deterministic; identical on all three targets)

Stack VM (gate off):

| Benchmark | Dispatches | Executable blocks | Operand HWM | Frames HWM |
|-----------|-----------:|------------------:|------------:|-----------:|
| fib(25) | 1,701,259 | 0 | 14 | 26 |
| calc_pi(1e6) | 1,775 | 1 | 4 | 3 |
| lorenz(1e6) | 1,780 | 1 | 4 | 3 |

Register VM (gate on; "residual" = stack VM dispatches still executed
around/inside the gated run):

| Benchmark | Register calls | Register dispatches | Residual stack dispatches | Fallback calls |
|-----------|---------------:|--------------------:|--------------------------:|---------------:|
| fib(25) | 121,393 | 971,141 | 851,510 | 2 |
| calc_pi(1e6) | 1 | 19,000,012 | 1,766 | 2 |
| lorenz(1e6) | 1 | 40,000,017 | 1,766 | 2 |

Key reading: the production stack VM does **not** interpret the two loop
benchmarks per instruction — its executable-block fast path
(`ExecutableBlock::Typed`) runs each 1M-iteration loop as **one** native
block (1.8k total dispatches). The register prototype replaces that block
with 19M/40M interpreted dispatches. `fib` has no executable block, so
there the comparison is interpreter vs interpreter (1.70M stack dispatches
vs 0.97M register + 0.85M residual; the engines alternate by call-tree
level through the trampoline).

### Wall time (median ms over 7 runs, `Vm::run()` only)

| Benchmark | Target | Stack VM | Register VM | Register/Stack |
|-----------|--------|---------:|------------:|---------------:|
| fib(25) | macOS host | 59.2 | 46.2 | **0.78×** |
| fib(25) | iOS Simulator | 64.0 | 46.8 | **0.73×** |
| fib(25) | Wasm/Node | 292.7 | 281.3 | ~0.96× (noisy) |
| calc_pi(1e6) | macOS host | 60.6 | 126.4 | 2.09× |
| calc_pi(1e6) | iOS Simulator | 60.1 | 126.8 | 2.11× |
| calc_pi(1e6) | Wasm/Node | 139.1 | 1,024.2 | 7.36× |
| lorenz(1e6) | macOS host | 114.9 | 179.0 | 1.56× |
| lorenz(1e6) | iOS Simulator | 115.7 | 185.1 | 1.60× |
| lorenz(1e6) | Wasm/Node | 348.1 | 885.9 | 2.54× |

Wasm samples spread ±15% (Node/JIT warmup variance); host/iOS spreads
were < 2%. Wasm artifact (`subset_julia_vm_web_bg.wasm`, web-release,
includes both engines + the #8559 bench entry): 11,254,161 bytes after
`wasm-opt`.

### Interpretation

- **Call-dominated code (fib): the register VM wins on host and iOS**
  (22–27% faster) despite paying a full stack-VM trampoline for every
  nested call and re-entering the register interpreter 121k times, and
  reaches ~parity on Wasm. Native register call frames (#8448 remaining
  scope) remove that tax and should widen the gap on all targets.
- **Typed loops (calc_pi, lorenz): the register prototype loses 1.6–2.1×
  on host/iOS and 2.5–7.4× on Wasm — but against native blocks, not
  against interpretation.** The stack VM's executable blocks collapse
  these loops into single native-code executions; the register VM
  interprets ~6.6 ns/dispatch on host yet still lands within ~2× of
  native there. On Wasm each dispatch costs ~54 ns (size-optimized
  web-release `match` loop lowered to `br_table`), so the 19M/40M
  dispatch counts dominate — confirming the #8448 caveat that on Wasm
  *reducing dispatch count* matters far more than micro-optimizing the
  dispatch loop. A register VM with loop superinstructions/block
  compilation would not pay this penalty, but that work does not exist
  yet.
- **The portable `match` loop is confirmed viable on all three targets**:
  identical deterministic counters everywhere, iOS Simulator within a few
  percent of host, Wasm functional and exactly output-compatible. No
  direct threading or computed goto was needed, matching the #8448
  dispatch-strategy decision; on Wasm the per-dispatch cost argues for
  fewer, fatter instructions rather than a different dispatch mechanism.

### Recommendation for #8446 Phase 3 (default switch): NOT YET

Do **not** make the register VM the default engine now. It beats the
stack VM only where the stack VM actually interprets (call-heavy shapes);
the production stack VM's executable blocks and call specialization
already run the hottest loop shapes as native blocks that the prototype
regresses ~2× on host/iOS and up to 7.4× on Wasm. Switching today would
trade a 25% win on recursion for a 2–7× loss on the loop-shaped numeric
code that dominates the iOS samples and the Web Playground.

Recommended order instead:

1. Native register call frames (drop the per-call trampoline) — extends
   the demonstrated fib-class win.
2. Loop superinstructions / executable-block equivalents on register
   bytecode (or SSA-driven fusion via #8440) — closes the calc_pi/lorenz
   gap at its root.
3. Re-run this matrix (harnesses are committed and deterministic); switch
   the default only when the register VM is ≥ stack VM on the
   executable-block loop shapes while keeping the recursion win.

## P0 Refresh Snapshot (Issue #9906, 2026-07-09)

As the first RegisterVM P0 refresh, `target/release/register_vm_bench_8559 7`
was run on current `main` (`5eb5adbb6`, Linux x86_64, release profile). This is
**not** the #9904 default-switch matrix: the current host is Linux, so iOS
Simulator cannot be run (`xcrun` unavailable), and the Wasm package cannot be
built here (`wasm-pack` unavailable). Keep the 2026-07-02 macOS/iOS/Wasm matrix
above as the last full cross-target decision record until P6 reruns every
target.

### Static Metrics

Instruction strides were unchanged: stack `Instr` = 72 B, `RegisterInstr` =
40 B, `Value` = 64 B.

| Body | Stack instrs | Stack bytes | Register instrs | Register bytes | Registers | Slots | Register frame bytes |
|------|-------------:|------------:|----------------:|---------------:|----------:|------:|---------------------:|
| `fib` | 11 | 792 | 13 | 520 | 2 | 1 | 192 |
| `calc_pi` | 28 | 2,016 | 28 | 1,120 | 4 | 4 | 512 |
| `lorenz_accum` | 52 | 3,744 | 55 | 2,200 | 2 | 10 | 768 |

### Dynamic Counters

Stack VM (gate off):

| Benchmark | Dispatches | Executable blocks | Operand HWM | Frames HWM |
|-----------|-----------:|------------------:|------------:|-----------:|
| `fib(25)` | 1,702,295 | 0 | 14 | 26 |
| `calc_pi(1e6)` | 2,802 | 0 | 5 | 3 |
| `lorenz(1e6)` | 2,802 | 0 | 5 | 3 |

Register VM (gate on; residual stack counters omitted here because they match
the stack-only loop dispatch counts on this Linux run):

| Benchmark | Register calls | Register dispatches | Fallback calls |
|-----------|---------------:|--------------------:|---------------:|
| `fib(25)` | 121,393 | 1,456,711 | 22 |
| `calc_pi(1e6)` | 1 | 23,000,016 | 22 |
| `lorenz(1e6)` | 1 | 56,000,018 | 22 |

Important reading: on this Linux host the stack executable-block counter is 0
for the loop benchmarks, so this snapshot does not reproduce the July 2
cross-target "one typed block" stack baseline. It still confirms the register
VM's immediate problem for #9904: loop-shaped workloads explode to tens of
millions of register dispatches while `fib` remains call-frame dominated.

### Wall Time

Median ms over 7 uninstrumented `Vm::run()` executions of the precompiled
program:

| Benchmark | Stack VM | Register VM | Register/Stack |
|-----------|---------:|------------:|---------------:|
| `fib(25)` | 184.655 | 126.152 | 0.68x |
| `calc_pi(1e6)` | 52.498 | 267.107 | 5.09x |
| `lorenz(1e6)` | 123.553 | 859.721 | 6.96x |

The output matched the upstream-Julia-pinned expected value on both engines for
all three benchmarks. P0 remains incomplete until the same current-main refresh
is run on iOS Simulator and Wasm/Node and until the dispatch-loop profile is
split by `Value` movement, slot/register traffic, and frame allocation.

## P1 Native Register Call Frames Snapshot (Issue #9904, 2026-07-09)

First P1 implementation slice: `RegisterInstr::CallStack` now runs translatable
callees as native register frames on an explicit interpreter frame stack. The
stack-VM trampoline remains the fallback for ineligible callees and intrinsics.

Host smoke command: `target/release/register_vm_bench_8559 3` on the same Linux
x86_64 host as the P0 snapshot. This is a short smoke measurement, not the P6
cross-target decision matrix.

| Benchmark | Stack VM median ms | Register VM median ms | Register/Stack |
|-----------|-------------------:|----------------------:|---------------:|
| `fib(25)` | 165.946 | 99.237 | 0.60x |
| `calc_pi(1e6)` | 52.600 | 325.988 | 6.20x |
| `lorenz(1e6)` | 117.133 | 971.059 | 8.29x |

Dynamic counters:

| Benchmark | Register calls | Register dispatches | Residual stack dispatches | Fallback calls |
|-----------|---------------:|--------------------:|--------------------------:|---------------:|
| `fib(25)` | 242,785 | 2,913,415 | 2,802 | 22 |
| `calc_pi(1e6)` | 1 | 23,000,016 | 2,802 | 22 |
| `lorenz(1e6)` | 1 | 56,000,018 | 2,802 | 22 |

Interpretation: P1 removes the per-recursive-call stack trampoline from `fib`
(residual stack dispatch drops from ~852k in the P0 Linux run to 2.8k), widening
the call-heavy register win. Loop-shaped workloads are intentionally not fixed
by P1; they still need P2 dispatch-count reduction and P3 loop-block execution.

## P2 Shared-Plan Slot-Fusion Slice (Issue #9906, 2026-07-09)

First P2 implementation slice: shared-plan lowering now emits typed slot
loads/stores, direct I64 slot compare false-branches, in-place `slot += const`
for exact `Int64` loop counters, in-place Float64-family `slot = -slot`, and
`BinF64Slot` for safe Float64 slot operands. This reuses the existing register
interpreter instruction set where possible and adds only the shared-plan
variants needed for slot-branch, slot-store, and slot-negation.

Release test counters on Linux host:

| Benchmark | Before P2 slice | After P2 slice | Reduction |
|-----------|----------------:|---------------:|----------:|
| `calc_pi(100000)` | 2,300,016 | 1,400,012 | 39.1% |
| `lorenz_accum(1e6)` | 56,000,018 | 39,000,015 | 30.4% |

Regression guards:

- `register_vm_parity_calc_pi_while_loop_issue_8558` asserts
  `dispatch_total <= 1,500,000`.
- `register_vm_forced_gate_lorenz_parity_issue_8559` asserts
  `dispatch_total <= 40,000,000`.

This is not the full P2 acceptance yet: the #9906 target is at least 2x
dispatch-count reduction on both `calc_pi` and `lorenz`. The remaining reduction
needs broader expression/assignment fusion and then P3 loop-block execution to
attack per-instruction dispatch overhead directly.

## P2 Literal/Slot Superinstructions (Issue #9906, 2026-07-09)

Second P2 implementation slice: shared-plan lowering now emits Float64
literal/slot and slot/slot superinstructions, plus fused Float64 binary stores
for assignments whose final operation stores directly back to a typed slot.
The fusions preserve Julia operand evaluation order: right-slot forms evaluate
the left expression first, store forms lower both operands before writing, and
literal-left forms only move literals across non-literal expressions.

Linux release regression counters:

| Benchmark | P2 baseline | After literal/slot slice | Reduction |
|-----------|------------:|-------------------------:|----------:|
| `calc_pi(100000)` | 2,300,016 | 1,100,011 | 52.2% |
| `lorenz_accum(1e6)` | 56,000,018 | 27,000,015 | 51.8% |

Regression guards now assert `calc_pi(100000) <= 1,150,000` and
`lorenz_accum(1e6) <= 28,000,000` register dispatches on the release test
surface. This satisfies the P2 Linux-host dispatch-count target by fusion
alone. Remaining P2/P6 work: verify deterministic counters on iOS Simulator and
Wasm/Node, then measure wall time; P3 loop-block execution is still needed for
the broader #9904 default-engine goal.

## P3 Register Loop Blocks, First Slice (Issue #9906, 2026-07-09)

First P3 implementation slice: after shared-plan lowering and jump patching,
RegisterVM recognizes the structural `while k < n` CFG shape emitted for typed
I64-slot loop guards:

`BranchCmpI64Slots(exit) -> body jump -> exit jump -> typed body -> backedge`.

The header is replaced with a `LoopI64Slots` dispatch token and the body is
stored in a side-table as a validated typed loop block. At runtime, the loop
hoists live-in Float64/Int64 slots into typed locals, executes the body against
typed locals, polls cancellation per 4096 iterations, then flushes slots back on
exit. The stored loop ops are resolved to block-local slot indexes up front, and
a post-pass fuses common Float64 slot-update chains such as
`load; slot-const binop; store` and `load; load; store`, so the hot loop does
not perform per-op slot-map lookup or copy large loop-op enum values. The
original instruction stream remains address-stable; the loop block sets `pc` to
the already-patched exit PC.

Linux release regression counters:

| Benchmark | After P2 literal/slot | After P3 loop block |
|-----------|----------------------:|--------------------:|
| `calc_pi(100000)` | 1,100,011 | 10 |
| `lorenz_accum(1e6)` | 27,000,015 | 14 |

Linux host smoke wall time (`target/release/register_vm_bench_8559 7`):

| Benchmark | Stack VM median ms | Register VM median ms | Register/Stack |
|-----------|-------------------:|----------------------:|---------------:|
| `fib(25)` | 325.749 | 201.836 | 0.62x |
| `calc_pi(1e6)` | 87.293 | 67.581 | 0.77x |
| `lorenz_accum(1e6)` | 196.936 | 139.257 | 0.71x |

This satisfies the P3 Linux-host loop direction for #9904's `calc_pi` and
`lorenz_accum` workloads. The remaining #9906 work is to run and record the full
host/iOS/Wasm P6 matrix before any default-engine decision.

## P6 Cross-Target Default-Switch Matrix (Issues #9904/#9906, 2026-07-10)

P6 reran the same `register_vm_bench_8559` workload on macOS host, iOS
Simulator, and Wasm/Node after the register gate was hardened for full fixture
parity. Commands:

- Host: `target/release/register_vm_bench_8559 7`
- iOS Simulator: `xcrun simctl spawn <iPad A16> target/aarch64-apple-ios-sim/release/register_vm_bench_8559 7`
- Wasm/Node: `node scripts/register_vm_wasm_bench_8559.mjs /tmp/sjulia-pkg-node-20260710-afterfix 7`

All runs matched the upstream-Julia-pinned output on both engines. Median wall
time in milliseconds:

| Benchmark | Target | Stack VM | Register VM | Register/Stack |
|-----------|--------|---------:|------------:|---------------:|
| `fib(25)` | macOS host | 74.225 | 54.561 | 0.73x |
| `calc_pi(1e6)` | macOS host | 32.273 | 20.996 | 0.65x |
| `lorenz_accum(1e6)` | macOS host | 67.490 | 40.263 | 0.60x |
| `fib(25)` | iOS Simulator | 74.455 | 62.011 | 0.83x |
| `calc_pi(1e6)` | iOS Simulator | 31.366 | 20.743 | 0.66x |
| `lorenz_accum(1e6)` | iOS Simulator | 66.798 | 40.565 | 0.61x |
| `fib(25)` | Wasm/Node 24.14.1 | 142.291 | 133.004 | 0.93x |
| `calc_pi(1e6)` | Wasm/Node 24.14.1 | 58.379 | 47.304 | 0.81x |
| `lorenz_accum(1e6)` | Wasm/Node 24.14.1 | 104.742 | 86.014 | 0.82x |

The static footprint remained unchanged from P3 on native targets:
`Instr=72B`, `RegisterInstr=40B`, `Value=64B`; the Wasm artifact was
12,644,671 bytes. Dynamic register counters were deterministic across targets:

| Benchmark | Register calls | Register dispatches | Residual stack dispatches | Fallback calls |
|-----------|---------------:|--------------------:|--------------------------:|---------------:|
| `fib(25)` | 242,785 | 2,913,415 | 2,803 | 22 |
| `calc_pi(1e6)` | 1 | 10 | 2,803 | 22 |
| `lorenz_accum(1e6)` | 1 | 14 | 2,803 | 22 |

Fixture parity was verified with
`SJULIA_REGISTER_VM=1 timeout 1800 cargo nextest run --release --test fixture_tests --jobs 1`
(`176 passed`). During the P6 acceptance run, two register-only parity hazards
were fixed:

- Issue #10047: the register gate now rejects functions whose return, parameter,
  slot, or remaining stack markers require unsupported non-`Int64`/`Float64`/
  `Bool`/`Nothing` semantics, so `Float32`, `BigInt`, global assignment, and
  dynamic-dispatch fixtures stay on the stack VM instead of being mis-boxed by
  partial register execution.
- Issue #10054: native register recursion now uses a catchable depth guard
  instead of growing the register frame stack until host OOM.

Go/no-go: #9904's performance acceptance is met on host, iOS Simulator, and
Wasm/Node for `fib(25)`, `calc_pi(1_000_000)`, and
`lorenz_accum(1_000_000)`, and #9906's P0-P6 roadmap has completed through the
cross-target measurement gate. A default-engine switch should still land as its
own release decision PR so the opt-in gate can be flipped with the normal
release, C ABI, and downstream app verification surface rather than bundled
into this benchmark/roadmap closure.

### Default-Switch Acceptance Checklist (Issue #10060)

The default-engine decision PR (and every PR that widens the register subset
before it) must keep these gates green, in addition to the normal pre-PR gates:

1. [ ] **Full fixture parity under the gate**:
   `SJULIA_REGISTER_VM=1 timeout 1800 cargo nextest run --release --test fixture_tests --jobs 1`
   — record the pass count here, as the P6 run above did (176/176).
2. [ ] Register-gate support matrix is intact: the exhaustive
   `instr_is_register_unsupported_stack_marker()` match in
   `subset_julia_vm_vm/src/vm/register_gate.rs` still has no `_` arm, and any
   `Instr` variants added since the last acceptance run carry an explicit
   register-gate classification (compile-enforced).
3. [ ] New supported types/ops each have a positive register execution test
   plus a negative/currently-stack-only test — see the "RegisterVM Feature
   Work Checklist" in `docs/vm/CHECKLISTS.md`.
4. [ ] Runaway recursion stays catchable under the gate
   (`error_stack_overflow*.jl` fixtures pass with `SJULIA_REGISTER_VM=1`,
   Issue #10054) — no host OOM from register frame growth.

## Stack VM Handling On Adoption (Issue #8639 cross-reference)

The backend-strategy ADR, `docs/vm/ADR_BACKEND_STRATEGY.md` (Issue #8639),
governs what happens to the **stack VM** if the register VM is promoted to
the default engine. Summary of the clause recorded there:

- Until the default switch, the stack VM is production and both engines
  stay inside the default test surface — a "maintain but do not verify"
  layer (the pre-ADR AoT state) is not permitted for either VM.
- On the default switch, the stack VM enters the freeze lifecycle the ADR
  defines as the general end-state for a superseded backend (the ADR chose
  *not* to apply it to AoT — AoT is grown, not frozen — but the lifecycle it
  describes is the template): a dated freeze decision appended to this file
  and to the ADR, new stack-VM-only superinstructions/fast paths frozen after
  3 months, and verification consolidated on a single scripted gate before
  any crate isolation.
- Permanent coexistence of both interpreters is explicitly not the default
  plan; keeping both verified per-PR forever would require its own recorded
  justification in the ADR.

## Handler-Table Dispatch Experiment (Issue #8562, 2026-07-02): DROP

Follow-up to #8446 Phase 1 item 3 ("dispatch-table / function-pointer
interpreter"): does replacing the stack VM's exhaustive-`match`
instruction dispatch with a handler-function-pointer table reduce
interpreter overhead? **Answer: no, on every target — the experiment is
dropped and the `match` loop stays the production dispatch.** This
resolves the #8446 Phase 1 dispatch-mechanism item negatively with data;
option (b) from that plan — keep the `match` and let LLVM emit a dense
jump table — is confirmed as the right shape, and dispatch-*count*
reduction (superinstructions PR #8512, executable blocks, the register
VM path above) remains the productive direction.

### Prototype

Gated behind the `vm-handler-table` cargo feature plus
`SJULIA_HANDLER_TABLE=1` / `set_handler_table_forced` (per-`Vm`, read at
construction; same gate contract as #8558/#8559). Default builds compile
none of it. `vm/exec/handler_table.rs` generates, from one macro list, a
dense row-index enum, a discriminant→row `match` (LLVM lowers the
constant-returning match to a jump-table lookup; Rust has no computed
goto and `std::mem::discriminant` cannot index an array), and a
`[fn(&mut Vm<R>, &Instr) -> Result<DispatchAction, VmError>; 123]` table
in matching order — dispatch is one table load plus one indirect call.
The 122 hot rows cover the numeric/loop/call subset the #8559 benchmarks
exercise (constants, slot load/store families incl. fused load-arith
forms, I64/F64 arithmetic/comparisons/conversions, the fused jump
family, direct calls, dynamic binary operators, returns); every other
instruction lands on a shared fallback row that re-enters the full
`match`, so coverage gaps change performance only, never semantics. Both
paths call the same `execute_*` group handlers (same
call-depth-overflow postludes); parity harness:
`tests/handler_table_parity_8562_tests.rs`.

### Method

Same harness pattern and benchmark set as #8559 (`fib(25)`,
`calc_pi(1_000_000)`, `lorenz_accum(1_000_000)`) plus
`calc_pi_call(1_000_000)` — a while loop whose per-iteration
user-function call keeps it on the per-instruction interpreter, because
the production executable-block fast path runs the two pure loops as
single native blocks (~1.8k total dispatches), leaving a dispatch
experiment almost nothing to act on. Host and iOS Simulator (iPad (A16),
iOS 26.1, `xcrun simctl spawn`) run `handler_table_bench_8562`
(`--release --features vm-handler-table`); Wasm runs
`subset_julia_vm_web::HandlerTableBench` under Node v24.14.1 via
`scripts/handler_table_wasm_bench_8562.mjs` (`wasm-pack --profile
web-release`; artifact with the gated feature compiled in: 11,586,901
bytes). Wall time = median of 7 uninstrumented `Vm::run()` executions of
a precompiled program (fresh `Vm` per run; counters disabled); output
byte-identical to upstream-Julia-pinned values on both paths on every
target. Counters are deterministic, identical across all three targets,
and asserted equal between the two paths (same instruction stream, only
the dispatch mechanism differs). Load caveat: another agent was building
on the measurement machine; the host run landed in a quiet window (1-min
load ~3–4) and within-run spreads stayed < 2% on host/iOS (Wasm ±4%),
matching the #8559 noise-robustness experience — the lead re-confirms
host wall numbers machine-quiet before quoting beyond the ratios.

### Deterministic counters (identical on host, iOS Simulator, Wasm)

| Benchmark | Dispatches | Exec blocks | Table hits | Fallback | Hot coverage |
|-----------|-----------:|------------:|-----------:|---------:|-------------:|
| fib(25) | 1,701,259 | 0 | 1,700,556 | 703 | 99.96% |
| calc_pi(1e6) | 1,775 | 1 | 1,072 | 703 | 60.4% |
| calc_pi_call(1e6) | 27,001,779 | 0 | 27,001,077 | 702 | 100.00% |
| lorenz(1e6) | 1,780 | 1 | 1,077 | 703 | 60.5% |

The ~700 fallbacks are top-level/startup instructions; on the two
dispatch-heavy benchmarks the hot rows serve 99.96–100% of dispatches,
so the wall-time deltas measure the dispatch mechanism itself, not
coverage gaps.

### Wall time (median ms over 7 runs, `Vm::run()` only)

| Benchmark | Target | `match` | Handler table | Table/Match |
|-----------|--------|--------:|--------------:|------------:|
| fib(25) | macOS host | 59.4 | 60.5 | 1.02× |
| fib(25) | iOS Simulator | 58.9 | 60.8 | 1.03× |
| fib(25) | Wasm/Node | 144.8 | 145.2 | 1.00× |
| calc_pi(1e6) | macOS host | 60.6 | 60.6 | 1.00× |
| calc_pi(1e6) | iOS Simulator | 60.1 | 61.4 | ~1.02× (noise) |
| calc_pi(1e6) | Wasm/Node | 76.8 | 76.9 | 1.00× |
| calc_pi_call(1e6) | macOS host | 446.8 | 462.2 | 1.03× |
| calc_pi_call(1e6) | iOS Simulator | 437.1 | 466.6 | 1.07× |
| calc_pi_call(1e6) | Wasm/Node | 1,220.0 | 1,319.5 | 1.08× |
| lorenz(1e6) | macOS host | 114.8 | 114.8 | 1.00× |
| lorenz(1e6) | iOS Simulator | 112.6 | 112.8 | 1.00× |
| lorenz(1e6) | Wasm/Node | 137.6 | 130.7 | ~0.95× (noise) |

### Interpretation and decision

- **Where dispatch actually dominates, the table loses everywhere**:
  +2–3% on the host, +3–7% on the iOS Simulator, up to +8% on Wasm
  (calc_pi_call: 27M dispatches at 100% hot coverage). The classic
  pitfall is confirmed — uniform `fn(&mut Vm, &Instr)` pointers defeat
  the per-arm inlining the `match` arms get, and the indirect call
  (`call_indirect` on Wasm) plus table load costs more than the dense
  jump table it replaces. LLVM already compiles the exhaustive `match`
  to one jump table (#6343), so the handler table buys no decode
  improvement to offset the lost inlining.
- **Where the executable-block fast path runs (calc_pi, lorenz), the
  mechanisms tie** — those loops execute as single native blocks
  (~1.8k dispatches), and the two sub-5% deltas in the table above
  (calc_pi iOS, lorenz Wasm) are directionally inconsistent
  block-dominated noise, not dispatch effects.
- **Wasm confirms the #8448 caveat**: both mechanisms lower to
  `br_table`/`call_indirect`, and the fn-pointer variant is strictly
  the more expensive encoding there. On Wasm the lever is reducing
  dispatch *count* (fatter instructions, block execution), not the
  dispatch mechanism.
- **Decision: DROP.** The default engine keeps the exhaustive `match`.
  The gated prototype and cross-target harnesses stay in-tree as the
  measurement record, and for cheap re-testing if the instruction
  representation ever changes shape (e.g. a byte-coded ISA whose real
  opcode bytes index a 256-row table with no discriminant mapping or
  bounds check). The #8446 Phase 1 "dispatch-table / function-pointer
  interpreter" item is resolved either way by this record.

## Remaining Scope

The prototype still falls back to stack VM trampolines for ineligible callees
and intrinsics, but translatable direct calls can now run as native register
frames. Cross-target measurements are published above (Issue #8559), and #8448
is closed because the decision record and target matrix are complete. The old
register-VM roadmap issue (#8446) is also closed; active default-switch work now
lives in #9904 ("Make register VM beat stack VM on loop-shaped workloads") and
the phased #9906 roadmap. Remaining work is ordered as P1 completion hardening
(frame storage reuse plus error/handler interaction coverage), P2 register
superinstructions, P3 loop-block execution, conditional P4 typed register banks,
P5 coverage expansion, and P6 cross-target remeasurement plus the default-switch
go/no-go decision. The related interpreter-overhead experiments (#8561/#8562)
are closed records.

## Value Enum Size Experiment: I128/U128 Boxing — REJECTED (Issue #8650)

**Decision date**: 2026-07-03
**Issues**: #8650 (parent), #8677 (prototype), #8678 (decision)

### What was tried

Boxing `Value::I128(i128)` and `Value::U128(u128)` to `Value::I128(Box<i128>)` and
`Value::U128(Box<u128>)`. This drops the enum's alignment requirement from 16 bytes
to 8 bytes, reducing the `Value` enum from 64B to 56B.

The prototype compiled cleanly (42 files changed). The size reduction was confirmed.
Benchmark files: `benchmarks/results/i128_boxing_baseline_8676.md` (baseline),
`benchmarks/results/i128_boxing_ab_results_8677.md` (A/B results).

### A/B Results (Criterion, 50 samples)

| Metric | Baseline (unboxed 64B) | Boxing (56B) | Change |
|--------|------------------------|--------------|--------|
| Gain-side: `vm_value_move_push_pop` | ~155.5 ms/iter | ~214.95 ms/iter | **+38.4% regression** |
| Loss-side: `vm_int128_arith` | ~58.3 ms/iter | ~11.65 ms/iter | −80% (improvement) |

### Decision: REJECTED

Pre-agreed formula (Issue #8676): ACCEPT if G ≥ 3% AND L ≤ 100%.

- **G = −38.4%** (gain-side regressed by 38%) → G < 3% → **REJECT**

The 56B non-power-of-2 enum size is cache-unfriendly for common-path push/pop of
non-I128 values. A 64B enum fits exactly in one cache line; a 56B enum does not.
The boxing prototype is preserved on branch `perf/8677-i128-boxing-prototype` for
reference but is not merged into main.

### Alternatives for #8448

To reduce `Value` size without the cache penalty, consider:
- Moving `StaticArrayInline` (40B payload) behind a registry handle — this is the
  largest remaining non-pointer variant and the most likely path to 48B or 40B.
- The 64B baseline is the current production size; any further reduction must show
  G ≥ 3% on the `vm_value_move_push_pop` benchmark before merging.

## Multi-Slot Scalar (isbits Immutable Struct) Unboxing (Issue #9198)

*Added 2026-07-06 (Issue #9198, slice 1 — design record + S2 allocation
baseline). This slice is design + measurement ONLY; no representation changes
land here. The slot-pair/SROA, array-storage, and Value-inline changes are S2–S6
below.*

This is the design record acceptance criterion 1 of Issue #9198 asks for: the
decision to make **isbits immutable struct unboxing** a first-class register-VM
requirement, reachable incrementally from the current stack VM. `Complex{Float64}`
is the driving case; the design generalizes to any small isbits immutable struct
(Design Principle 10 — general over ad-hoc).

### Why (the representation the last week of Complex fast paths exposed)

An sjulia struct value carries its type on **three independent faces** —
`StructInstance { type_id: usize, struct_name: Rc<str>, values: Vec<Value> }`
(`subset_julia_vm_bytecode/src/value/struct_instance.rs:61`). A tag/payload
*mismatch* is therefore representable, and #9167 produced exactly one (a value
whose `struct_name`/`type_id` said `Complex{Int64}` while the fields were `F64`).
Separately, even a 2-field isbits struct heap-allocates a `Vec<Value>` for its
fields on **every** operation result: #9125 shared the *name* `Rc<str>` but not
the field `Vec`, so a typed `z = z*z + c` loop still allocates per iteration. The
Rust fast path `try_complex_f64_binary_op`
(`subset_julia_vm_vm/src/vm/exec/binary_both.rs:653`) intercepts the dynamic
`Complex{Float64}` route but still returns
`Value::Struct(StructInstance::complex_with_shared_name(…))` — one `Vec` alloc —
and its hand-written *F64-only* gate is precisely what #9167 mis-set. "Boxed
representation is slow → add a Rust fast path → mis-gate it" ran a full cycle in a
week; the structural fix is the representation, not more Layer-2 code.

### Upstream shape (`./julia`)

`typeof` upstream is a **single** `jl_datatype_t*` in the value header
(`jl_taggedvalue_t`, `julia/src/julia.h`); the field memory layout is *derived*
from that datatype's `jl_datatype_layout_t`. A value whose tag and payload
disagree cannot be constructed without memory corruption. Because
`Complex{Float64}` is isbits (immutable + all fields isbits), upstream stores it
**unboxed**: contiguous in arrays (16 B/element, no pointer), register/stack
expanded for locals/arguments (no boxing), and its arithmetic is the **pure
`julia/base/complex.jl`** at native speed — upstream never needs a C/Rust
specialization of complex arithmetic *because the representation (unboxing) is the
general solution*.

The array-storage rule is `jl_get_genericmemory_layout`
(`julia/src/datatype.c:511`), which calls `jl_islayout_inline(eltype, &elsz, &al)`
and branches three ways:

1. **inline, non-union isbits eltype** → contiguous **unboxed** elements of size
   `elsz` (aligned to `al`), `npointers` derived from the element layout;
2. **inline union** → unboxed payload **plus a per-element selector byte**;
3. **not inline** → a **boxed pointer array** (`npointers = 1`, one pointer per
   element).

sjulia's array plan (Design B) mirrors case 1 for isbits structs.

### Design A — slot-pair / SROA for typed-loop locals (S2, S3)

When inference proves a typed local is a small isbits struct (the 2-field
`Complex{Float64}` first), lower it to a **group of scalar slots** (`f64 × 2`)
instead of one boxed `Value::Struct` slot. This is scalar replacement of
aggregates (SROA): `z = z*z + c` then reads/writes the four `f64` slots and never
constructs a `StructInstance`, so the typed loop issues **zero per-iteration heap
allocations** — the S2 acceptance target. It is a natural extension of the
existing slotization pass, not a new engine:

- **`subset_julia_vm_bytecode/src/slot.rs`** — the slot table
  (`build_slot_info` / `slotize_code`, see `docs/vm/SLOTIZATION.md`). Today one
  name → one slot holding a whole `Value`. Slot-pair generalizes: one
  isbits-struct name → *k* typed scalar slots (k = field count), and field access
  (`real(z)`/`imag(z)`) becomes a slot-index offset instead of a `GetField` on a
  boxed struct.
- **`subset_julia_vm_bytecode/src/slot_metadata.rs`** — per-slot type metadata;
  the home for "this slot group is the SROA'd form of struct type `T` with field
  layout […]" — the slot-level analogue of upstream's `jl_datatype_layout_t`.
- **`subset_julia_vm_bytecode/src/peephole.rs`** — already fuses typed
  load/store/arith into the `Load{Add,Sub,Mul,Div}F64Slot` / `LoadSquareF64Slot`
  families (see the register-VM coverage list above). Complex `*`/`+` on a slot
  pair lowers to **those existing f64 fused ops**, so the SROA'd loop reuses the
  current typed-f64 instructions rather than adding intrinsics (Principle 3, Pure
  Julia First — the Julia `base/complex.jl` bodies inline down to f64 slot ops).
- **Register VM** — the "Frame Layout" section above already reserves typed
  register storage as the next step "once measurements show which values are worth
  unboxing." The isbits slot-pair is exactly that: slot-pair on the stack VM and
  typed registers on the register VM are the *same* SROA lowering emitted from the
  shared typed SSA (`Core IR → SSA IR`), so it is done once, not per backend.

Slot-pair needs **no runtime type tag**: the slot group's type is compile-time
metadata, so it does not depend on #9197. (Design C does.)

### Design B — contiguous isbits array storage (S4, S5)

The array side has three tiers today, and the general user-isbits path is the
boxed one:

| Tier | Representation | Boxing |
|------|----------------|--------|
| Complex special case | `ArrayData::F64(Vec<f64>)` + `ArrayElementType::ComplexF64` (interleaved `[re,im,…]`, `array_element.rs:18`) | **unboxed** (16 B/elt) but keyed on a Complex-only eltype tag |
| isbits struct AoS | `ArrayElementType::StructInlineOf(type_id, field_count)` backed by `ArrayData::Any(Vec<Value>)` (`array_value/mod.rs:1067`) | de-structed (no `struct_heap` chase) but each field is still a boxed 64-B `Value` |
| general user struct | `ArrayData::StructRefs(Vec<usize>)` = indices into `struct_heap` (`array_data.rs:180`) | **fully boxed** + pointer chase |

Plan: add a genuinely **byte-contiguous** `ArrayData` variant whose element layout
is *derived from the struct definition* (mirrors upstream case 1), so
`Vector{Point{Float64}}` stores raw `[x1,y1,x2,y2,…]` f64 rather than `Vec<Value>`.
**S5** then folds the `ComplexF64` interleaved buffer into that general mechanism —
Complex becomes "the 2×f64 isbits struct," retiring the eltype special case
(Principle 10). The logical element type stays in **one** place,
`MemoryValue.element_type` (`memory_value.rs`) / `ArrayValue.element_type_override`,
matching proposal 3's "type in one location ⇒ #9167-class tag/payload mismatch is
unrepresentable." Deriving the layout from the struct definition is the sjulia
analogue of `jl_islayout_inline` + `jl_datatype_layout_t`, and makes
`sizeof(Vector{T})` match upstream (acceptance criterion 5).

### Design C — the `Value`-inline variant question (open; sequenced after #9197)

Off the typed-slot path (dynamic dispatch, heterogeneous containers) a boxed small
isbits struct still costs a heap `Vec`. A `Value` variant carrying `(type tag,
inline field bytes)` would make it `Copy` and heap-free there too. This is not
hypothetical: **`Value::StaticArrayInline(StaticArrayInlineData)`** already does
exactly this for N≤4 `SVector`/`SMatrix` (#7964 P3, `static_real.rs:257`) — a 40 B,
`Copy`, zero-alloc payload. A 2-field isbits struct `(tag, f64, f64)` is ~24 B.

The constraint is the `Value` enum ceiling: 64 B, 53 variants, alignment 16 (from
inline `I128`/`U128`), max payload **48 B** (`Struct`/`Pairs`/`NamedTuple`/
`Function`); the audit `test_value_enum_size_is_compact`
(`value_enum.rs:920`, #8005) fails if a new variant pushes past 64 B. A 24 B inline
struct payload fits comfortably.

The blocker is the **type tag**: an inline variant needs a compact tag that
identifies the *concrete* type **including parameters**. A raw `type_id: usize`
cannot (it conflates `SubArray{Int64,1}` with `SubArray{Float64,2}`, and
`Complex{Int64}` with `Complex{Float64}` — the #9167 family). That tag is exactly
**`ConcreteTypeId(u32)`** from `docs/vm/TYPE_INTERNING.md` (#9197 S1 landed the
intern registry; later slices make it dispatch identity). Design C is therefore
**sequenced after #9197** delivers the parameter-inclusive tag; Design A (slot-pair)
is independent and can proceed first.

### Relationship to the Rust fast paths — retirement (S6)

The #9125 / #9154 Complex Rust fast paths (`binary_both.rs`, `dynamic_ops/`) are
**transitional**. Once Design A removes per-iteration boxing on the typed loop and
#9116 (Pure-Julia `AbsF64` etc., currently blocked because "dispatch is slower than
the Rust path" — the same boxed-slowness root cause) unblocks, the fast paths are
retired under an **interleaved A/B measurement** (acceptance criterion 4). This
record fixes their status as "temporary until the representation is fixed."

### Allocation baseline (S2 acceptance baseline)

Measured by `subset_julia_vm/tests/complex_loop_allocation_baseline_9198_tests.rs`
— a **test-only counting global allocator** (justified: the existing VM memory
stats, `REPLSession::last_vm_memory_stats` → `struct_heap_len` /
dispatch-cache-entry counts used by `session_boundedness_8625_tests.rs`, measure
*steady-state resident* cache/heap **entry counts per eval**, not the transient
heap allocations issued *while a loop runs*; a boxed `Complex{Float64}` field
`Vec` never lands in `struct_heap`, so no existing counter observes it). The
counter is windowed around `Vm::run()` and **differenced across iteration counts**
(N=2000 vs 4000) so parse/lower/compile/Base-specialization cancel:
`allocs_per_iter = (allocs(4000) − allocs(2000)) / 2000`.

Numbers are provisional/local (NS-4), Apple Silicon macOS host, `release-fast`,
2026-07-06:

| Loop (typed function, `z = z*z + c`) | Heap allocs / iteration |
|--------------------------------------|-------------------------|
| **`Complex{Float64}` (boxed, current)** | **21** |
| real-decomposed `Float64` control (2 slots) | 1 (the interpreted-loop floor) |
| **≈ allocations attributable to the boxed struct** | **~20** |
| **S2 target for the Complex loop** | **≤ 1 (match the control)** |

The real-decomposed control loop bottoms out at 1 heap alloc/iteration — the
current per-iteration floor for an interpreted (non-executable-block) typed loop,
*not* struct-related. The boxed `Complex{Float64}` loop pays **21**, so ~20
allocs/iter are attributable purely to constructing/cloning the boxed
`StructInstance` (its 2-element field `Vec`) on each `*`/`+` result and slot
load/return. Driving the Complex loop down to the control's level (≤1/iter) is the
S2 acceptance bar ("typed complex loop → zero per-iteration heap allocations from
the struct representation").

Wall-clock, `cargo bench -p subset_julia_vm --bench vm_complex_arith_benchmark`
(`mandelbrot_complex_run_only`, `Vm::run()` only, precompiled program, machine
quiet):

| Benchmark | Median (current, boxed) |
|-----------|-------------------------|
| `vm_complex_arith/mandelbrot_complex_run_only` (30×20×25, 5519 inner iters) | **40.20 ms** ([40.13, 40.26], 100 samples, < 0.4% spread) |

Mandelbrot ComplexF64-vs-real-decomposed runtime gap (acceptance criterion 3,
target 10x → ≤2x). Supplementary cold-CLI cross-check, same mandelbrot count at
60×40×100 (= 64097, identical on all three), `release-fast` sjulia vs upstream
`julia` 1.12.6, min of 4 runs; the sjulia startup floor (78 ms, `println(0)`
probe) is subtracted to approximate VM work — an approximation, **not** a VM-only
measurement (see the note on cold CLI above):

| Program (60×40×100) | Cold CLI (min) | ≈ VM work (− startup floor) |
|---------------------|---------------:|----------------------------:|
| sjulia `Complex{Float64}` mandelbrot | 504 ms | **~426 ms** |
| sjulia real-decomposed mandelbrot | 84 ms | **~6 ms** (near the typed-loop / noise floor) |
| upstream `julia` complex / real | 144 ms / 143 ms | compute effectively free (native, unboxed) |

The ~426 ms boxed-Complex VM estimate cross-checks the criterion `Vm::run()`
median (40.2 ms at 30×20×25 scales by ~16× in workload to the same order). The
real-decomposed form runs the loop as near-native typed blocks in single-digit ms,
so boxed Complex is **> 10×** slower here — consistent with (and at this size
larger than) the 10× #8796 reported, and driven by the 21 : 1 per-iteration
allocation ratio above. Upstream `julia` runs *both* forms with effectively free
compute (its `base/complex.jl` is unboxed at native speed), which is exactly the
target this epic reaches for. The committed same-size ComplexF64-vs-real *A/B*
(with the fast path A/B) is the S2/S6 measurement record after the representation
lands; this slice fixes the allocation baseline (the cause) and the boxed runtime
(the effect) as the before-numbers.

### Slice roadmap (the contract S2–S6 code against)

| Slice | Deliverable | Acceptance / contract |
|-------|-------------|-----------------------|
| **S1 (this record)** | design record + allocation baseline; no representation change | REGISTER_VM.md section (this) + `complex_loop_allocation_baseline_9198_tests.rs` counter; `cargo check` clean |
| **S2 (landed, PR pending)** | slot-pair SROA for typed `Complex{Float64}` loop locals (`compile::complex_sroa`) | typed `z=z*z+c` loop → **1 per-iteration heap alloc** (was 21), matching the real-decomposed control's interpreter floor (baseline test flips); output byte-identical to upstream; reuses `slot.rs`/`slot_metadata.rs`/`peephole.rs` f64 fused ops (`LoadSquareF64Slot`/`LoadMulF64Slot`/`LoadAddF64Slot`); no new instruction |
| **S3 (landed)** | generalize slot-pair beyond `Complex{Float64}`-only: (a) `im`-literal inits with provably-`Float64` coefficients, (b) a boxed `::ComplexF64` param used as a decomposed operand, (c) any user 2-field `Float64` immutable struct | the mandelbrot kernel spelling (`z = 0.0+0.0im`; `c::ComplexF64` param) now fully unboxes; a user `struct V2{x::Float64,y::Float64}` construct/field-read loop unboxes to slot pairs (`compile::complex_sroa`); no new intrinsics; user structs recognized structurally from `StructDef` (no type-name special-casing, Principle 8/10) |
| **S4 (landed)** | byte-contiguous isbits `ArrayData` variant, layout derived from the struct def | `Vector{Vec2{Float64,Float64}}(undef,n)` stores raw interleaved f64 (`ArrayData::StructF64`, not `Vec<Value>`); `sizeof` matches upstream (`n*field_count*8`: `Vector{Vec2}`=48/3elts, `Vector{Complex{Float64}}`=80/5elts, criterion 5); mirrors `jl_get_genericmemory_layout` case 1 |
| **S5 (landed)** | fold the `ComplexF64` interleaved **buffer** into the S4 general `StructF64` storage | `Complex{Float64}` arrays back their interleaved `[re,im,…]` data with the general contiguous-isbits `ArrayData::StructF64` variant (shared with user 2×f64 structs), not the scalar-`Float64` `ArrayData::F64`; the `ComplexF64` element-type **tag is kept as the entry point** for is_complex/matmul/display/inference; byte-identical (`sizeof(Vector{Complex{Float64}})` stays `16·n`). Deferred to S6: retiring the `ComplexF64` *eltype tag* itself (Complex tagged `StructInlineF64`) and the `ComplexF32` f32-buffer analogue |
| S6 | retire the #9125/#9154 Rust fast paths under interleaved A/B measurement; delete the `ComplexF64`/`ComplexF32` array *eltype tags* (Complex tagged `StructInlineF64`; needs an f32 struct-buffer variant) | fast-path removal recorded with before/after numbers (criterion 4); #9116 unblocked |

Design C (`Value`-inline isbits variant) is gated on #9197's `ConcreteTypeId` tag
and is scheduled after S2–S3 prove the slot-pair path; it is tracked here as the
off-typed-slot complement, not a numbered slice yet.

### S2 as landed (`compile::complex_sroa`)

S2 implements Design A as a **source-to-source Core IR rewrite** (the sjulia
analogue of Julia's SROA), not a bytecode-level pass — but the target it lowers to
is exactly the `slot.rs`/`slot_metadata.rs`/`peephole.rs` f64 fused-op family the
design named, so the mechanism matches. The pass runs on the user segment after
`ir_opt` (`compile::pipeline_ctx::inline_and_optimize_ir`) and, per function,
splits every *stably*-`Complex{Float64}` local into two `f64` locals (`re`/`im`),
decomposing `z = z*z + c`, `real`/`imag`/`abs2`, `z.re`/`z.im`, `+=`, subtraction,
`ComplexF64 ⊕ Real`, and `conj` into real `f64` arithmetic; the rewritten IR then
compiles through the existing typed-`f64` slot machinery (the loop body becomes
`LoadSquareF64Slot` / `LoadMulF64Slot` / `LoadAddF64Slot` / `SubF64`, identical to
the hand-written real-decomposed control). **No `Instr` variant was added.**

**SROA gate — what qualifies / what bails (correctness-first).** A local is
unboxed only when *every* assignment to it provably yields `Complex{Float64}`
(constructor `Complex{Float64}(a,b)`/`ComplexF64(a,b)` forces F64; `ComplexF64 ⊕
ComplexF64` stays F64; `ComplexF64 ⊕ Real` promotes the real into F64 —
`promote_type(Complex{Float64}, Real) == Complex{Float64}`), reached through a
concrete constructor (grounding excludes ungrounded self-referential cycles). Any
other occurrence of the local is **materialized** back to
`Complex{Float64}(re, im)` at the boundary (return, `push!`, a call argument,
interpolation), so escapes are correct by construction. The pass **bails to the
original boxed form** (never miscompiles) on: closure capture / `global` /
non-`Assign` binding of the local; a `let`/`AssignExpr`/quote or comprehension
binding that shadows it; and — deliberately, this slice — `im`-based literals
(`0.0im`, `1 + 2im`: their element type depends on the coefficient type, so a
naive f64 unboxing would be unsound) and complex/complex (and real/complex)
*division* (Julia's numerically-careful Smith algorithm would not be reproduced
bit-for-bit by the naive formula). Parameters are not SROA'd (they arrive boxed).
The `*` decomposition preserves upstream's exact op order (`zr*wr - zi*wi`,
`zr*wi + zi*wr`), so results are byte-identical (verified against `julia` 1.12.6 in
`tests/fixtures/complex/complex_slot_pair_sroa_9198.jl` and mirrored unit tests in
`compile::complex_sroa`). S3 (#9198) generalizes this to any 2-field (then
k-field) isbits struct with no type-name special-casing.

**Measured (Apple Silicon macOS, `release-fast`, 2026-07-06; NS-4 provisional).**

| Metric | Before (boxed) | After (S2 slot-pair) |
|--------|---------------:|---------------------:|
| `z=z*z+c` loop heap allocs / iter (`complex_loop_allocation_baseline_9198_tests`) | **21** | **1** (= real-decomposed control's interpreter floor; struct-attributable allocs → 0) |
| `vm_complex_arith/mandelbrot_complex_run_only` `Vm::run()` median | **40.20 ms** | **≈1.72 ms** (~23×; [1.711, 1.723], idle host) |

Acceptance criterion 3 (mandelbrot ComplexF64-vs-real gap, target 10× → ≤2×) is
met and exceeded: the SROA'd complex loop compiles to the *same* fused-f64 slot
ops as the real-decomposed form, so the gap collapses to ≈1× (both run as typed
f64 blocks). The #9125/#9154 Rust fast paths remain in place — their retirement
(with the interleaved A/B measurement, criterion 4) is S6.

### S3 as landed (`compile::complex_sroa` — generalization)

S3 lifts the S2 `Complex{Float64}`-only gate along three axes (Design Principle 10),
keeping the same slot-pair mechanism and adding **no `Instr` variant**. The pass now
recognizes a **shape** — a 2-field `Float64` isbits struct — per split local, and
decomposes accordingly:

1. **`im`-literal initializers with provably-`Float64` coefficients.** `z = 0.0 +
   0.0im` / `c = cr + ci*im` lower to `Add(a, Mul(k, im))`; `im` is
   `Complex{Bool}(false,true)`, so `k*im` is `Complex{Float64}(0.0, k)` **only when
   `k` is provably `Float64`** (`is_provably_f64`: float literal / `Float64(…)` /
   f64 arithmetic). `2im` (`Complex{Int64}`) and any non-provably-f64 coefficient
   still **bail** (stay boxed) — the element-type soundness that S2 conservatively
   protected by bailing on all `im` forms is preserved by the coefficient gate.
2. **A boxed `::Complex{Float64}` parameter as a decomposed operand.** A parameter
   is still not split, but when it appears in a decomposable expression
   (`z = z*z + c`, `c::ComplexF64`), its `re`/`im` are read via field access and
   **hoisted to two `f64` locals at function entry** (`__cx_re_c = c.re;
   __cx_im_c = c.im`), so the loop reads f64 slots, not the boxed param. This is
   what makes the **mandelbrot acceptance-kernel spelling** (`mandel_point(c::ComplexF64,
   …)` with `z = 0.0 + 0.0im`; `mandelbrot_acceptance_aot.jl`) fully unbox through
   the interpreter VM — its `z=z*z+c` body compiles to exactly the fused
   `LoadSquareF64Slot`/`LoadMulF64Slot`/`LoadAddF64Slot`/`SubF64` sequence, no boxed
   `StructInstance`, no `NewStruct`. (AoT codegen uses a separate IR path that does
   not run this pass, so the `aot` gate is unaffected by S3.)
3. **Any user 2-field `Float64` immutable struct**, recognized **structurally** from
   its `StructDef` (non-parametric, immutable, no inner constructor, exactly two
   concrete `Float64` fields) — *not* by type name (Principle 8/10). Construction
   `V2(a, b)`, field reads `p.x`/`p.y`, var copies, and escape materialization
   decompose. `Complex{Float64}` stays recognized by constructor spelling only
   because it is a *parametric* Base type whose `::T` fields cannot be read as
   "2×f64" off a `StructDef` without instantiating `T`; its **arithmetic** rules are
   Complex semantics, not a dispatch shortcut. **Honest scope:** user structs have
   no built-in arithmetic, so an operator method call (`p + q`, a user `+(::V2,::V2)`)
   is **not** inlined by this pass and stays boxed — the S3 user-struct win is
   confined to the **construct + field-read** shape (`p = V2(p.x+1, p.y+2)`), which
   is exactly where a boxed `NewStruct`/`Vec` alloc per iteration is removed. A
   constructor arg that is already provably `Float64` skips the `convert(Float64,…)`
   so the field-arithmetic loop is byte-for-byte the hand-written real-decomposed
   form.

Still bailed (documented, tracked for a later slice): mixed `Int`/`Float` `im`
coefficients (`1.0 + 2im` — Complex{Float64} upstream but not proven here),
`k`-field / mixed-type isbits structs, parametric user structs, complex/complex and
real/complex division (Smith algorithm), and user-struct operator inlining.

**Measured (Apple Silicon macOS, `dev-fast`/`release-fast`, 2026-07-06; NS-4
provisional).**

| Form (new in S3) | Before (boxed) | After (S3 slot-pair) |
|------------------|---------------:|---------------------:|
| `im`-literal init loop (`z=0.0+0.0im`) heap allocs / iter (`complex_loop_allocation_baseline_9198_tests`) | ~21 (S2 left it boxed) | **1** (= control floor) |
| mandelbrot kernel `mandel_point(c::ComplexF64,…)`, `z=0.0+0.0im`: loop body | boxed `Call *`/`Call +`, `z::Struct`, per-iter `NewStruct` alloc | fully fused f64 slot ops (`LoadSquareF64Slot`/`LoadMulF64Slot`/`LoadAddF64Slot`/`SubF64`), `c.re`/`c.im` hoisted; **zero per-iter alloc** |
| user `V2{x::Float64,y::Float64}` `p=V2(p.x+1,p.y+2)` loop body | `GetField`/`AddF64`/`NewStruct` (1 alloc/iter) | `LoadSlotF64`/`AddF64`/`StoreSlotF64` (no alloc, no `Float64` convert) |

The S2 `vm_complex_arith/mandelbrot_complex_run_only` bench (local-`c` constructor
form, already SROA'd at S2) is unchanged by S3 — it is the **regression guard** that
the generalized shape logic did not perturb the Complex path.

### S4 as landed (contiguous isbits array storage)

S4 implements Design B's array side. Two append-only variants carry the layout:

- **`ArrayData::StructF64(Vec<f64>)`** (`value/array_data.rs`) — the genuinely
  byte-contiguous store: an all-`Float64` isbits struct array holds interleaved
  raw f64 (`[x1,y1,x2,y2,…]`), *not* a `Vec<Value>` of boxed fields. Never
  bincode-serialized (arrays are not cache constants), so no wire impact.
- **`ArrayElementType::StructInlineF64(type_id, field_count)`** (`array_element.rs`,
  declared LAST for append-only bincode stability) — the logical eltype tag,
  carried in **one** place (`element_type_override` / `MemoryValue.element_type`),
  so a #9167-class tag/payload mismatch is unrepresentable. It is embedded in
  serialized `Instr` payloads and `array_element.rs` is a Base-cache schema-file,
  so appending bumps the schema fingerprint; `CACHE_VERSION` 86 → 87 makes that
  explicit.

The **routing decision is structural** (`StructDefInfo::inline_f64_field_count`,
`metadata.rs`): a struct qualifies iff it is immutable, isbits, and *every* field
is `Float64` — no type-name special-casing (Principle 8/10). This generalizes the
S2/S3 SROA'd 2×f64 shape to any N-field all-`Float64` immutable struct
(`Complex{Float64}`, user `Vec2`/`Vec3`, …). The runtime chokepoint
`array_element_type_from_julia_type_resolved` (`vm/exec/array_basic.rs`) returns
`StructInlineF64` instead of the boxed `StructOf` there, so `Vector{T}(undef,n)` /
`Memory{T}(undef,n)` / `fill`/`zeros`/`similar` all get contiguous storage.
getindex/setindex!/iterate/`collect`/`map`/`push!`/broadcast are value-parity with
upstream; getindex reconstructs the concrete *named* struct via a VM-synced
thread-local `type_id → name` registry (`struct_instance::set_struct_name_registry`,
refreshed at `Vm::run`) so `show`/`typeof` of an element match a heap-boxed struct.
Heap `StructRef` store operands are resolved to inline `Value::Struct` at the VM
store boundaries (`resolve_struct_ref_for_inline_store`: `MemorySet`,
`memoryrefset!`, IndexStore/`push!` grow paths) since the bytecode-crate storage
layer has no `struct_heap`. **No `Instr` variant was added.**

**Acceptance (`sizeof`, criterion 5) — matches upstream Julia 1.12 exactly:**

| Array | upstream `sizeof` | sjulia `sizeof` (S4) | note |
|-------|------------------:|---------------------:|------|
| `Vector{Complex{Float64}}(undef, 5)` | 80 | **80** | `5 × 16` (already contiguous pre-S4) |
| `Vector{Vec2{Float64,Float64}}(undef, 3)` | 48 | **48** | `3 × 16` — was `3 × 8 = 24` boxed pre-S4 |
| `Vector{Vec3{…3×Float64}}(undef, 2)` | 48 | **48** | `2 × 24` — general N-field |

Contiguity is pinned by `struct_inline_f64_array_is_byte_contiguous_no_box_9198`
(the layout test asks for: raw f64 buffer of `n·field_count` slots, no per-element
box) plus the fixture `array/isbits_struct_contiguous_9198.jl` (julia-parity).

**Scoped-out for a later slice (correct, just not yet contiguous):** the typed
literal `Vec2[…]` and `[Vec2(…) for …]` comprehension stay on the boxed `StructOf`
compile path (they resolve `Vector{T}` annotations that also feed #9188/#9133
typed-field codegen; routing them needs the literal-build `StructRef` resolution
too) — values and `eltype` are upstream-correct, only their `sizeof` is the boxed
`n·8`. Mixed-type / non-`Float64` isbits structs likewise stay boxed
(`StructInlineF64` is the all-f64 family; the fully general per-field byte-buffer
layout is the natural S4+ extension). S5 folds the `ComplexF64` interleaved special
case into this general mechanism.

### S5 as landed (Complex{Float64} arrays fold onto `StructF64` storage)

S5 retires the **storage** half of the `ComplexF64` array special case (design
table tier 1). Before S5, a `Complex{Float64}` array stored its interleaved
`[re0,im0,re1,im1,…]` buffer in `ArrayData::F64` — the *scalar*-`Float64` variant,
reinterpreted 2-slots-per-element and disambiguated only by the `ComplexF64`
element-type override (the "keyed on a Complex-only eltype tag" fragility). S5
routes that buffer through the S4 general contiguous-isbits variant
**`ArrayData::StructF64`** instead, so there is **one** contiguous-isbits array
buffer shared by `Complex{Float64}` and user all-`Float64` structs. `ArrayData::F64`
is now exclusively real `Vector{Float64}`. This mirrors upstream's distinct
`jl_datatype_layout_t` for `Float64` vs `Complex{Float64}` memory.

- **What folded** (`Complex{Float64}` only): the construction sites
  (`capacity_data_for`/`undef_data_for`/`complex_f64`, `array_value/mod.rs`), the
  get/set/push/pop/`set_complex` interleaved arms (`array_value/access.rs`,
  `array_value/mutation.rs`), the `similar` builder (`vm/builtins_arrays.rs`), and
  the broadcast fast-path read (`vm/broadcast.rs`, via a new storage-agnostic
  `ArrayValue::complex_interleaved_f64` accessor) all now use `ArrayData::StructF64`.
- **What is kept as the entry point** (deferred to S6): the
  `ArrayElementType::ComplexF64` *tag* stays — it is what `is_complex`,
  `is_complex_array` (matmul / scalar·array), the `Complex{Float64}` array-show
  prefix, `eltype`/`typeof`, and the compiler's element-type inference key on.
  Deleting the tag (tagging Complex arrays `StructInlineF64` and reconstructing via
  the `type_id→name` registry) is a larger, higher-risk change gated to S6.
- **`Complex{Float32}` is unchanged**: it keeps interleaved `ArrayData::F32`
  (`StructF64` is f64-only). Its analogue needs an f32 struct-buffer variant — S6.
- **Byte-identical.** getindex still reconstructs via `complex_from_storage`
  (`struct_name` "Complex{Float64}"); `sizeof(Vector{Complex{Float64}})` stays
  `16·n` (size is computed from the logical eltype, not the buffer). Verified by
  `complex/complex_array_contiguous_storage_9198_s5.jl` (julia-parity, 36 asserts)
  and the full complex/linalg/broadcast fixture set + the mandelbrot AoT kernel.

Incidental bug filed (kept byte-identical, not worked around): #9492 —
`push!(Complex{Float64}[], z)` on an *empty* typed complex array errors because the
`ComplexF64` push arm does not resolve a heap `StructRef` (the S4 `StructInlineF64`
store path already does; the fix likely falls out of routing complex stores through
it in S6).

### S6 as landed (fast-path retirement — measured decision: KEEP)

S6 is the epic's terminal slice: it applies the Performance Decision Protocol
(CHECKLISTS.md) to the #9125/#9154 hand-written `Complex{Float64}` Rust fast paths
(`vm/exec/binary_both.rs::try_complex_f64_binary_op`, `vm/dynamic_ops::try_complex_f64_int_pow`)
that were the transitional stopgap for per-op boxing. With S2/S3 SROA (typed scalar
loops never box) and S4/S5 contiguous arrays (isbits array storage) landed, the
question is whether the residual **dynamic-dispatch** Complex route (non-SROA'd
locals, `sum` over complex arrays, materialized `z^n`) still needs them.

**Decision formula (fixed before measuring):** retire the fast paths iff removing
them regresses the residual dynamic Complex route by **≤5%**; otherwise KEEP with
the numbers recorded.

**Measurement** — a process-wide `complex_fastpath_gate` (`AtomicBool`, `Relaxed`,
default `false` = fast paths active = shipping behaviour; mirrors the `#8559`
register-VM override) lets the `vm_complex_dynamic` criterion bench (`benches/`)
flip the fast paths off and measure the general-dispatch fallback A/B. Medians
(provisional — measured under multi-agent contention, but the delta is far outside
any noise band):

| bench | with fast path | without | ratio |
|---|--:|--:|--:|
| `dyn_pow` (`z^n` dynamic) | 240 µs | 682 µs | **2.8× slower** |
| `array_sum` (`sum(::Vector{Complex})`) | 141 µs | 502 µs | **3.6× slower** |

**Decision: KEEP.** Removing the fast paths is a 180–260% regression on the residual
dynamic route — far past the 5% retirement bar. SROA/contiguous-arrays cover the
*static* Complex paths structurally, but the *dynamic-dispatch* Complex route still
depends on the #9125/#9154 fast paths, so they are load-bearing, not vestigial. The
measurement is the deliverable: the fast paths stay, now with a permanent A/B
regression guard (`vm_complex_dynamic`) so a future refactor that accidentally
disables them is caught. The gate read is a single `Relaxed` atomic load — negligible
next to the fast path's own `struct_heap` lookup.

This closes Issue #9198: the isbits-unboxing epic delivered scalar SROA (S2/S3),
contiguous isbits arrays (S4/S5), and a measured, evidence-backed decision to retain
the dynamic-route Rust fast paths (S6). Deferred, tracked separately: the
`ComplexF64` *eltype-tag* retirement and the `Complex{Float32}` f32-buffer analogue
(design table, S5 notes) — representation cleanups, not part of the unboxing win.
