# SubsetJuliaVM — Architecture Overview

*Last updated: 2026-06-11*

This document is the single entry point for new contributors. It explains the high-level
architecture of SubsetJuliaVM and links to detailed per-topic documents in `docs/vm/`.

---

## 1. What is SubsetJuliaVM?

SubsetJuliaVM is a **static pipeline** that executes a strict subset of Julia on iOS (and WebAssembly) **without a JIT compiler**. The output must match official Julia byte-for-byte.

```
Julia source
    │
    ▼
┌─────────┐   ┌──────────┐   ┌──────────────┐   ┌────┐
│  Parser │ → │ Lowering │ → │   Compiler   │ → │ VM │
└─────────┘   └──────────┘   └──────────────┘   └────┘
                                                   │
                              Swift/iOS via C ABI ◄┘
```

Each stage is implemented in Rust and lives in `subset_julia_vm/src/`.

---

## 2. Pipeline: Stage by Stage

### 2.1 Parser (`src/parser/`)

Converts Julia source text into a **Concrete Syntax Tree (CST)**.

- Implemented by the pure Rust `subset_julia_vm_parser/` crate (lexer / parser / CST) — a faithful reimplementation of the `tree-sitter-julia` grammar with no tree-sitter runtime dependency. `src/parser/` is the thin layer that connects the parser crate to the VM pipeline.
- Produces typed CST nodes such as `Expr::BinaryOp`, `Stmt::FunctionDef`, etc.
- See `docs/vm/PARSER.md` and `docs/vm/LOWERING.md`.

### 2.2 Lowering (`src/lowering/`)

Transforms the CST into **Core IR** — a simplified, normalized representation.

Key responsibilities:
- Expand syntactic sugar (short-form functions, `.=` broadcast, `where` clauses)
- Parse type annotations and build `JuliaType` nodes
- Collect nested function definitions and closure captures
- Surface lowering errors (unsupported syntax) with precise source spans

Key entry point: `Lowering::lower(parse_outcome: ParseOutcome) -> LowerResult<Program>` in `lowering/mod.rs` (the `include`-aware variant is `LoweringWithInclude::lower`)

Detailed doc: `docs/vm/LOWERING.md`

### 2.3 Compiler (`src/compile/`)

Converts Core IR into a flat sequence of **VM bytecode instructions** (`Instr`).

Sub-stages inside the compiler:

| Phase | File(s) | Responsibility |
|-------|---------|----------------|
| Method table construction | `compile/mod.rs` | Register all function signatures |
| Abstract interpretation | `compile/abstract_interp/` | Shared lattice-based type inference engine for function return and call-site refinement |
| Inference trace (developer) | `compile/inference_trace.rs` | `infer_with_trace(...)` entrypoint — runs inference on a single function and returns a `TraceReport` with per-statement env snapshots, branch envs, recursive-cycle events, and the final return type. Analogous to Julia's `typeinf_code` (Issue #3512). Disabled by default; thread-local opt-in collector keeps the regular compilation path zero-cost. |
| Core compilation | `compile/core_compiler.rs`, `compile/stmt.rs`, `compile/expr/` | Emit bytecode per expression/statement |
| Type inference (expression-level) | `compile/expr/infer/` | Determine `ValueType` for each expression |
| Dispatch selection | `compile/expr/binary/`, `compile/expr/call/` | Choose static vs. runtime dispatch |
| Constant propagation | `compile/const_prop/` | Evaluate constants at compile time |
| Union splitting | `compile/union_split/` | Split dispatch on union types |
| Effect inference | `compile/effects/` | Infer side-effect properties |
| Transfer functions | `compile/tfuncs/` | Type-level transfer functions (11 files); each entry is a metadata-bearing `TransferRule` carrying `min_arity`, `max_arity`, and `cost`, mirroring Julia's `add_tfunc(f, minarg, maxarg, tfunc, cost)` (Issue #3509) |
| Type stability analysis | `compile/type_stability/` | Detect type-unstable code paths (6 files) |
| Precompiled Base cache | `compile/precompile.rs`, `compile/cache.rs` | Skip recompiling Base at startup |

Output: `CompiledProgram` (a `Vec<Instr>` + metadata)

### 2.4 VM (`src/vm/`)

Interprets `CompiledProgram` using a **stack-based bytecode VM**.

Key components:

| File / Directory | Responsibility |
|-----------------|----------------|
| `vm/mod.rs` | `Vm<R>` struct — top-level execution loop |
| `vm/exec/` | Per-instruction execution handlers (35 files, organized by operation category) |
| `vm/value/` | `Value` enum — runtime representation of all Julia values |
| `vm/frame.rs` | Call frame (locals, return address) |
| `vm/instr.rs` | `Instr` enum — all bytecode instructions |
| `vm/executable.rs` | Conservative predecoded executable blocks for hot typed loops; recognizes small typed bytecode loops and runs them with loop-local scalar slots before falling back to the stack interpreter |
| `vm/builtins_macro/` | Macro evaluation (eval, parse, helpers, IR conversion) |
| `vm/builtins_reflection/` | Type/struct introspection (primitives) |
| `vm/builtins_sets/` | Set operations (set_ops, intrinsics, shared) |
| `vm/builtins_*.rs` | Other built-in function implementations |
| `vm/matmul/` | Matrix multiplication (multiply, scalar, complex, helpers) |
| `vm/hof_exec/` | Higher-order function execution (6 files) |
| `vm/type_ops/` | Runtime type operations (comparison, conversion, introspection, iteration, deep_copy) |
| `vm/dynamic_ops/` | Dynamic dispatch helpers (dispatch, helpers) |
| `vm/specialize/` | Value specialization (expr, stmt, helpers) |
| `vm/formatting.rs` | `format_value()` / `format_sprintf()` for output |
| `vm/error.rs` | `VmError` / `SpannedVmError` — runtime error variants with source spans |

Detailed docs: `docs/vm/CALL_INSTRUCTIONS.md`, `docs/vm/PANIC_FREE.md`

---

## 3. Pure Julia vs. Rust Boundary

SubsetJuliaVM uses a **three-layer architecture** (see `docs/vm/PURE_JULIA_DESIGN.md`):

```
Layer 3 — Pure Julia   subset_julia_vm/src/julia/
           Types, operators, promotion, math, collections, broadcast, IO

Layer 2 — Rust VM      subset_julia_vm/src/vm/
           Dispatch machinery, display, array ops, built-ins

Layer 1 — Rust Intrinsics   subset_julia_vm/src/intrinsics.rs
           CPU instructions (add_int, mul_float, …), hash table internals
```

**Rule**: Implement in Pure Julia (Layer 3) first. Add a Rust built-in only when the
operation cannot be expressed in Julia (e.g., memory primitives, OS interfaces).

When is a new Rust built-in justified?
- Reading a file (`open`, `read`, `close`)
- Hashing (`hash(::Int64)`)
- CPU-level arithmetic intrinsics

See `docs/vm/BUILTIN_REMOVAL.md` for the migration strategy when removing Rust built-ins
in favour of Pure Julia.

---

## 4. Type System: Three Representations

SubsetJuliaVM uses **three distinct type representations** at different stages.
Each is a lossy projection of the full Julia type system.

```
Compile-time                     Bytecode boundary          Runtime
─────────────────────────────    ──────────────────    ──────────────────
LatticeType                          ValueType              Value
(abstract interpretation)          (bytecode slots)     (tagged union)
```

### 4.1 `LatticeType` (compile-time)

Location: `compile/lattice/types.rs`

Used by the abstract interpreter. Supports:
- `Bottom` — unreachable code
- `Const(v)` — statically-known constants
- `Concrete(T)` — a single known type
- `Union{T1,T2,...}` — set of possible types
- `Conditional` — flow-sensitive narrowing
- `Top` — unknown / Any

### 4.2 `ValueType` (bytecode level)

Location: `vm/value/value_enum.rs`

Used in bytecode instructions. Simpler than `LatticeType` — ~49 variants covering
primitive types (`I64`, `F64`, `Bool`, …), collection types (`Array`, `Dict`, …),
struct types (`Struct(type_id)`), and `Any`.

`ValueType::Any` means "the concrete type is not known at compile time"; the VM
will perform a runtime dispatch.

### 4.3 `JuliaType` (compiler intermediate)

Location: `types/julia_type/mod.rs`

Used inside the compiler to track the Julia-level type of an expression before it
is lowered to `ValueType`. Includes parametric variants (`VectorOf(Box<JuliaType>)`,
`TupleOf(Vec<JuliaType>)`, `Struct(name)`) that `ValueType` collapses.

### Mapping

```
JuliaType::Int64        →  ValueType::I64     →  Value::I64(i64)
JuliaType::Float64      →  ValueType::F64     →  Value::F64(f64)
JuliaType::Struct("P")  →  ValueType::Struct(id) → Value::StructRef(Rc<RefCell<StructInstance>>)
JuliaType::Any          →  ValueType::Any     →  Value::* (any variant)
```

Detailed doc: `docs/vm/TYPE_SYSTEM.md`, `docs/vm/NUMERIC_TYPES.md`

---

## 5. Dispatch System

### 5.1 Overview

Function calls go through one of three paths:

```
Call site
   │
   ├─ Static dispatch ──────► Instr::Call(global_index, nargs)
   │   (types fully known,      direct function lookup
   │    no ambiguity)
   │
   ├─ Dynamic single-arg ───► Instr::CallDynamic(…)
   │   (one arg is Any)          runtime method scoring
   │
   └─ Dynamic multi-arg ────► Instr::CallTypedDispatch(…)
       (multi-arg unknown)        scored matching across table
```

### 5.2 Scored Dispatch

At runtime, when a dynamic instruction executes, the VM scores each candidate method
through the **shared dispatch resolver** in `inference_core/dispatch_resolver.rs`
(Issue #3910 migrated this out of the per-handler code in `vm/util.rs`). Per-argument
scores come from `CoreType::dispatch_pattern_score()` in
`inference_core/type_core/match.rs`:

| Score | Condition |
|-------|-----------|
| 4 | Exact type match |
| 3 | Type-variable parametric match |
| 2 | Base name / array family match |
| 1 | Subtype match (fallback via `check_subtype`) |

`runtime_type_pattern_score()` sums the per-argument scores; the highest-scoring
method wins, and ties keep the first candidate.

**Important**: Any new dynamic dispatch handler MUST go through the shared resolver
(`resolve_runtime_type_pattern_candidates*()` / `resolve_callable_value_candidates()`)
— never write inline scoring. See `docs/vm/BINARY_DISPATCH.md` and
`docs/vm/CALL_INSTRUCTIONS.md`.

### 5.3 Binary Operator Dispatch

Binary operators (`+`, `-`, `*`, etc.) have two parallel code paths:

- **Compile-time**: `compile/expr/binary/` — selects `CallDynamic*` vs. builtin instruction
- **Runtime**: `vm/exec/call_dynamic_binary.rs` (one operand `Any`), `vm/exec/binary_both.rs` (both `Any`), `vm/exec/binary_no_fallback.rs` (user methods shadow builtins) — execute the chosen instruction

Both paths must stay in sync. Detailed doc: `docs/vm/BINARY_DISPATCH.md`.

---

## 6. Module Structure

```
ailujsoi/
├── subset_julia_vm/           # Core library crate
│   └── src/
│       ├── parser/            # CST parser (tree-sitter-julia)
│       ├── lowering/          # CST → Core IR
│       ├── ir/                # Core IR types (Expr, Stmt, Block)
│       ├── compile/           # Core IR → bytecode (Instr)
│       │   ├── abstract_interp/  # Lattice-based type inference
│       │   ├── expr/             # Expression compilation
│       │   │   ├── call/            # Function call compilation (dynamic, module_call, nary)
│       │   │   ├── binary/          # Binary op compilation (builtin, user_defined)
│       │   │   └── infer/           # Expr-level type inference (julia_type, array, hof)
│       │   ├── lattice/          # LatticeType, ConcreteType
│       │   ├── ipo/              # Inter-procedural optimization
│       │   ├── const_prop/       # Constant propagation (eval)
│       │   ├── union_split/      # Union type splitting (detection, env_split, merge, specialize)
│       │   ├── effects/          # Effect inference (inference, propagation)
│       │   ├── tfuncs/           # Transfer functions (11 files: arithmetic, array_ops, etc.)
│       │   └── type_stability/   # Type-stability analysis (6 files)
│       ├── inference_core/    # Shared type/dispatch core (CoreType, subtype engine,
│       │                      #   dispatch_resolver.rs — shared scored dispatch, Issue #3910)
│       ├── vm/                # Bytecode interpreter
│       │   ├── exec/             # Instruction handlers (35 files by operation category)
│       │   ├── value/            # Value enum + sub-types
│       │   ├── builtins_macro/   # Macro evaluation (eval, parse, helpers, ir_conversion)
│       │   ├── builtins_reflection/  # Type/struct introspection
│       │   ├── builtins_sets/    # Set operations
│       │   ├── builtins_*.rs     # Other built-in function implementations
│       │   ├── matmul/           # Matrix multiplication
│       │   ├── hof_exec/         # Higher-order function execution (6 files)
│       │   ├── type_ops/         # Runtime type operations (comparison, conversion, etc.)
│       │   ├── dynamic_ops/      # Dynamic dispatch helpers
│       │   └── specialize/       # Value specialization (expr, stmt, helpers)
│       ├── repl/              # REPL support (session, converters, globals)
│       ├── types/             # JuliaType definition
│       │   └── julia_type/       # JuliaType module
│       ├── julia/             # Pure Julia source (base/, stdlib/)
│       ├── aot/               # Ahead-of-Time compilation support
│       │   ├── abi.rs            # Backend-neutral boxed/unboxed ABI boundary
│       │   ├── analyze/          # Core IR analysis (core_ir_analyzer, ir_converter/, loader)
│       │   ├── codegen/          # Code generation (aot_codegen/, cranelift/)
│       │   ├── ir/               # AoT intermediate representation (basic_types, aot_types, ops)
│       │   ├── inference/        # Type inference engine (types, engine/)
│       │   ├── native_calls.rs   # ccall/llvmcall boundary classification
│       │   ├── pass_pipeline.rs  # Named AoT pass diagnostics and verifier hooks
│       │   ├── rooting.rs        # Runtime Value rooting/safepoint contract
│       │   └── optimizer/        # Optimization passes (8 files: constant_folding, CSE, DCE,
│       │                         #   inlining, loop_opt, strength_reduction)
│       └── ffi/               # C ABI for Swift/iOS
├── subset_julia_vm_runtime/   # AoT bytecode runtime crate
├── SubsetJuliaVMApp/          # SwiftUI iOS app
├── mobile/                    # Flutter app
├── docs/vm/                   # Architecture documentation
└── scripts/                   # CI audit scripts
```

### Key `docs/vm/` References

| Topic | Document |
|-------|----------|
| Type system | `TYPE_SYSTEM.md` |
| Type inference | `TYPE_INFERENCE_COMPLETE.md` |
| Lowering / CST | `LOWERING.md` |
| Call instructions | `CALL_INSTRUCTIONS.md` |
| Binary dispatch | `BINARY_DISPATCH.md` |
| Pure Julia design | `PURE_JULIA_DESIGN.md` |
| Builtin removal | `BUILTIN_REMOVAL.md` |
| Collections | `COLLECTIONS.md` |
| Numeric types | `NUMERIC_TYPES.md` |
| AoT native calls | `AOT_NATIVE_CALLS.md` |
| AoT rooting / safepoints | `AOT_ROOTING_SAFETY.md` |
| Panic-free VM | `PANIC_FREE.md` |
| Status / Done | `STATUS.md`, `DONE.md` |

---

## 7. Key Data Structures

### `Value` (runtime)

Location: `vm/value/`

The `Value` enum is the universal Julia value at runtime. All stack slots, locals, and
globals hold a `Value`. Major variants:

```rust
enum Value {
    I64(i64), I32(i32), I16(i16), I8(i8),
    U64(u64), U32(u32), U16(u16), U8(u8),
    I128(i128), U128(u128), BigInt(…), BigFloat(…),
    F64(f64), F32(f32), F16(f16),
    Bool(bool),
    Str(String), Char(char), Symbol(SymbolValue),
    Nothing, Missing, Undef,
    NativeArray(ArrayRef),             // Compatibility N-dimensional array carrier
    Memory(MemoryRef),                 // Flat typed memory buffer (Memory{T})
    MemoryRef(Box<MemoryRefValue>),    // MemoryRef{T}
    Range(RangeValue), SliceAll,
    Struct(StructInstance),
    StructRef(usize),                  // Heap index into vm.struct_heap
    Rng(RngInstance),
    Tuple(TupleValue),
    SimpleVector(TupleValue),
    NamedTuple(NamedTupleValue),
    Pairs(PairsValue),
    Dict(DictRef),
    Set(SetValue),
    Ref(RefCellRef),
    DataType(JuliaType),
    RuntimeTypeVar(Box<RuntimeTypeVarValue>),
    Module(Box<ModuleValue>),
    IO(IORef),
    Function(FunctionValue),
    Closure(ClosureValue),
    ComposedFunction(ComposedFunctionValue),
    Expr(ExprValue), QuoteNode(Box<Value>),
    LineNumberNode(LineNumberNodeValue), GlobalRef(GlobalRefValue),
    Regex(RegexValue), RegexMatch(Box<RegexMatchValue>),
    Enum { type_name: String, value: i64 },
    // … more
}
```

### `Instr` (bytecode)

Location: `vm/instr.rs`

The bytecode instruction set. Instructions are stack-based; the VM maintains an
evaluation stack in `vm/mod.rs`. Examples:

```rust
enum Instr {
    PushI64(i64),            // push literal
    Add,                     // pop two, push sum
    Call(global_index, nargs), // static call
    CallDynamic(name, nargs, candidates), // scored dynamic dispatch
    Return,
    JumpIfZero(offset),      // conditional branch
    // … ~400 variants total (many are typed/fused specializations)
}
```

### `Frame` (call stack)

Location: `vm/frame.rs`

One frame per function invocation. Contains:
- Local variable slots — boxed `Value` slots (`locals_slots`) plus unboxed typed slot
  vectors (`slot_i64`, `slot_f64`, `slot_bool`, …) for slotized locals
  (see `docs/vm/SLOTIZATION.md`)
- Return address (instruction index)
- Function name (for error messages)

### `CompiledProgram`

Location: `vm/types.rs`

Output of the compiler. Key fields (among others — global slot metadata,
show-method registry, specializable functions, …):
- `code: Vec<Instr>` — flat bytecode (all functions concatenated)
- `functions: Vec<FunctionInfo>` — function metadata (start index, arity, name)
- `struct_defs: Vec<StructDefInfo>` — user-defined struct layouts
- `entry: usize` — bytecode index of the main script entry point

### `CoreCompiler<'a>`

Location: `compile/core_compiler.rs`

The main compilation state. Borrows:
- `method_tables` — registered Julia methods (dispatch tables)
- `shared_ctx: SharedCompileContext` — struct defs, global types, show methods

Emits bytecode into `self.code: Vec<Instr>`.

---

## 8. Adding New Features (Checklist)

When implementing a new Julia built-in or language feature:

1. **Find the official implementation** in `julia/base/` or `julia/stdlib/`
2. **Reproduce in Pure Julia** at `subset_julia_vm/src/julia/` (same path)
3. **Add fixture tests** — run `julia tests/fixtures/<category>/test.jl` first to verify
4. **Check dispatch** — does the new function need a dynamic handler? See `BINARY_DISPATCH.md`
5. **Update docs** — add to `STATUS.md` (done), or `UNIMPLEMENTED.md` (not yet)
6. **Run CI** — `timeout 1800 cargo nextest run --release`

See `CLAUDE.md` for the full workflow, code audit rules, and git conventions.
