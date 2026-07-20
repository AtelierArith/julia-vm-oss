# Pure Julia Design: Implementation Architecture

This document describes the Pure Julia implementation strategy in SubsetJuliaVM, documenting the boundary between Pure Julia code and Rust infrastructure. The design follows CLAUDE.md principles #3 ("Write Pure Julia First") and #4 ("Avoid New Rust Intrinsics Unnecessarily").

*Last updated: 2026-06-26 (Issue #7879: resolved Set/Dict/Array carrier-retirement drift; the `Value::Dict`/`Value::Set`/`Value::Array` carriers were fully retired by #6731/#6732/#4568).*

## Design Philosophy

SubsetJuliaVM implements types and functions using a layered architecture:

```
Layer 3: Pure Julia    型定義, 演算, promotion, 数学関数, コレクション, ブロードキャスト
Layer 2: Rust VM       ディスパッチ機構, 表示, 配列演算, 組み込み関数
Layer 1: Rust Intrinsic  CPU命令 (add_int, sdiv_int, mul_float, ...), ハッシュテーブル
```

The goal is to maximize Layer 3 (Pure Julia) while minimizing Layers 1 and 2. Domain logic should be Pure Julia; Rust provides only infrastructure that cannot be expressed in Julia (CPU primitives, OS interfaces, hash table internals).

### Compile-time cost: in-loop closures threaded into recursive helpers (Issues #8182/#8185)

Pure-Julia code is free at runtime but not at **compile time**: every Base/package
function without a declared return type is run through interprocedural return-type
inference when its module loads (`compile.build_method_tables`). One Pure-Julia
pattern is pathological for this analysis — **a closure defined inside a loop and
passed as an argument into a deep (mutually-)recursive call tree**. Under the loop
fixpoint, the whole call tree is re-specialized for each concrete closure, so
inference work grows super-linearly even though the result is always correct and
runtime is fast. `Optim._bfgs` (a `phidphi` closure threaded through the
HagerZhang line search `hagerzhang_search → _hz_secant2! → _hz_update! →
_hz_bisect!`) made `using Optim` ~5.5 s, 97 % of it in `build_method_tables`.

Mitigation when writing such a function in Pure Julia (Base or a bundled package):

- Give it an **exact declared return type** (`f(...)::T`) so `build_method_tables`
  skips inferring the body (the #7215 / #8182 short-circuit). Verify exactness
  against upstream `julia`.
- The engine has a `MAX_INTERPROCEDURAL_ANALYSIS_WORK` backstop that widens to
  `Top` on a *catastrophic* blow-up, but it is deliberately not the package-load
  performance mechanism. Historical measurements showed unannotated
  `using Symbolics` at ≈159k work and the `_bfgs` blow-up at ≈174k work — the
  same order, so the backstop could not discriminate them. The annotations, plus
  per-package load-time smoke tests, are the real guards.
- See `docs/vm/CHECKLISTS.md` → "New Bundled Package / New Solver — Load-Time
  Inference Check" before adding a new solver or bundled package.

## Public Base Dispatch Policy

Public `Base` names should resolve through method dispatch whenever the
operation is expressible in Pure Julia. Rust remains below that layer as either
an underscored intrinsic used by Pure Julia methods, a primitive fallback for
legacy VM value representations, or a boundary that cannot be expressed inside
sjulia today (OS, native ABI, parser/runtime internals, regex, GMP/MPFR, linear
algebra backends).

This policy is intentionally stricter for structured Pure Julia values:
`Dict{K,V}` structs, iterator wrappers, tuples, ranges, `Complex`, `Rational`,
and user structs should not be captured by public-name Rust shortcuts. The
legacy Rust-backed carriers `Value::Dict`, `Value::Set`, and `Value::Array`
have since been **fully retired** (Issues #6731/#6732/#4568); the only remaining
references to those names are unreachable error strings and explanatory
comments. Public `Dict`/`Set`/`Array` behavior now selects ordinary methods on
the parametric Pure Julia structs with no legacy-carrier fallback.

The same rule applies to module-qualified public calls. `Base.length(x)`,
`Base.keys(x)`, `Base.getindex(x, i)`, and other migrated public names are
forwarded into the ordinary `compile_call` method-dispatch path before Rust
builtin fallback routing is considered (Issue #3861). This keeps explicit
`Base.foo` calls from bypassing Pure Julia methods or user-defined extensions.

## Pure Julia Codebase Overview

The Pure Julia codebase currently spans **76 files** totaling **~43,000 lines**
under `subset_julia_vm/src/julia/`:

| Directory | Files | Lines | Contents |
|-----------|-------|-------|----------|
| `base/` | 65 | ~38,500 | Core library: types, operators, math, collections, broadcast, strings, IO |
| `stdlib/` | 10 | ~3,800 | Standard libraries: LinearAlgebra, Dates, Test, Random, Printf, etc. |
| `internal/` | 1 | ~600 | AoT prelude |

### Largest Pure Julia Files

| File | Lines | Contents |
|------|-------|----------|
| `base/iterators.jl` | 5,231 | Iterator protocol, combinators (zip, enumerate, product, etc.) |
| `base/array.jl` | 4,303 | Array{T} struct, operations, comprehensions |
| `base/broadcast.jl` | 2,856 | Broadcast infrastructure (BroadcastStyle, Broadcasted, materialize) |
| `stdlib/LinearAlgebra/src/LinearAlgebra.jl` | 1,829 | LinearAlgebra subset |
| `base/reflection.jl` | 1,583 | Reflection and type-query helpers |
| `base/io.jl` | 1,583 | IO operations and show methods |
| `base/subarray.jl` | 1,203 | SubArray/view support |
| `base/rational.jl` | 1,119 | Rational{T} type and arithmetic |
| `base/promotion.jl` | 1,092 | Type promotion rules and convert methods |
| `base/range.jl` | 1,037 | Range types (UnitRange, StepRange, LinRange) |
| `base/strings/util.jl` | 973 | String utility functions |
| `base/error.jl` | 969 | Exception types and error handling |

## Pure Julia Coverage by Domain

```
Rational:    ████████████████████████████████████████████████░░  ~98%
Complex:     ████████████████████████████████████████░░░░░░░░░░  ~75%
Trig/Exp/Log:██████████████████████████████████████████████████  100% (Float64)
Dict{K,V}:   ████████████████████████████████████████████████░░  ~95% (struct dispatch; Value::Dict carrier retired #6731)
Set:         ████████████████████████████████████████████████░░  ~95% (pure-Julia Dict{T,Nothing} wrapper #6721/#6732)
Broadcast:   ████████████████████████████████████████████████░░  ~95%
Array{T}:    ███████████████████████████████████████████░░░░░░  ~85% (110+ Pure Julia methods over Memory{T}; Value::Array carrier retired #4568)
```

| Domain | Pure Julia functions | Rust special-case sites | Key files |
|--------|---------------------|------------------------|-----------|
| Rational | ~60 | 2 | `rational.jl` |
| Complex | ~120 | ~40 | `complex.jl`, `special/trig.jl` |
| Trig/Exp/Log | ~22 | 0 | `special/trig.jl`, `special/exp.jl`, `special/log.jl`, `math.jl` |
| Dict{K,V} | ~36 | 0 carrier (struct dispatch; #6731) | `dict.jl` |
| Set | ~23 | 0 carrier (pure-Julia `Dict{T,Nothing}` wrapper; #6721/#6732) | `set.jl` |
| Broadcast | ~100+ | ~10 (array fast paths) | `broadcast.jl` |
| Array{T} | ~110 | creation/mutation primitives over `Memory{T}`; `Value::Array` carrier retired (#4568) | `array.jl`, `abstractarray.jl`, `iterators.jl` |
| Math | ~77 | varies | `math.jl` |

## Computation Flow: `1 + 3 // 2`

### Operator Precedence

`1 + 3 // 2` is parsed as `1 + (3 // 2)` because `//` has higher precedence than `+`.

| Implementation | `+` precedence | `//` precedence |
|---|---|---|
| Julia (`julia-parser.scm`) | `prec-plus` | `prec-rational` (between `*` and `<<`) |
| SubsetJuliaVM (`precedence.rs`) | `Plus = 21` | `Rational = 23` |

### Step-by-Step Execution

```
"1 + 3 // 2"
     │
     ▼
[Parser] 3 // 2
  Julia本家: (call // 3 2)             token: //
  SubsetJuliaVM: BinaryExpression       token: SlashSlash
     │
     ▼
[Lowering] // → 関数呼び出し
  Julia本家: //(n,d) = Rational(n,d)    (通常の関数)
  SubsetJuliaVM: Expr::Call{"//",[3,2]} (BinaryOpではなくCall)
     │
     ▼
[実行] 3 // 2 → Rational{Int64}(3, 2)
  GCD正規化: gcd(3,2)=1 → 既に既約
  結果: Rational{Int64}(num=3, den=2)
     │
     ▼
[実行] 1 + Rational{Int64}(3, 2)
  VM: should_use_inline_dynamic_op(I64, Struct) → false
      → find_best_method_index(&["+"], &[I64, Rational{Int64}])
      → Julia dispatch
  Julia: promote(1, 3//2) → (1//1, 3//2)
         +(1//1, 3//2) = Rational(1*2+3*1, 1*2) = Rational(5, 2)
     │
     ▼
  結果: 5//2  ✓ (本家と同一)
```

### Key Design Decision: `//` as `Expr::Call`

The `//` operator is lowered to `Expr::Call` (function call), **not** `BinaryOp` enum. This differs from `/` which maps to `BinaryOp::Div`. The reason: `//` is a user-definable function in Julia, while `/` is a compiler intrinsic.

```rust
// lowering/expr/binary.rs:141-151
if op_text == "//" {
    return Ok(Expr::Call {
        function: "//".to_string(),
        args: vec![left, right], ...
    });
}
```

## Computation Flow: `1 + 3.0im`

### Juxtaposition (Implicit Multiplication)

`3.0im` is **not** a special literal — it is juxtaposition (implicit multiplication): `3.0 * im`.

| Implementation | Mechanism |
|---|---|
| Julia (`julia-parser.scm`) | `juxtapose?` predicate: number + no space + identifier → `(call * 3.0 im)` |
| SubsetJuliaVM (`postfix.rs`) | `JuxtapositionExpression` CST node → `BinaryOp::Mul` in lowering |

### The `im` Constant

| Implementation | Definition |
|---|---|
| Julia (`base/complex.jl:36`) | `const im = Complex(false, true)` — runtime global variable |
| SubsetJuliaVM (`lowering/expr/mod.rs:71-77`) | Hardcoded at lowering time as `Literal::Struct("Complex{Bool}", [false, true])` |

### Step-by-Step Execution

```
"1 + 3.0im"
     │
     ▼
[Parser] 3.0im → 並置式
  Julia本家: (call * 3.0 im)
  SubsetJuliaVM: JuxtapositionExpression(FloatLiteral, Identifier)
     │
     ▼
[Lowering]
  SubsetJuliaVM: BinaryOp::Mul(3.0, Complex{Bool}(false, true))
     │
     ▼
[実行] 3.0 * im
  *(x::Real, z::Complex) = Complex(x * real(z), x * imag(z))
  = Complex(3.0 * false, 3.0 * true) = Complex{Float64}(0.0, 3.0)
     │
     ▼
[実行] 1 + Complex{Float64}(0.0, 3.0)
  VM: should_use_inline_dynamic_op(I64, Struct) → false
      → Julia dispatch
  +(x::Real, z::Complex) = Complex(x + real(z), imag(z))
  = Complex(1 + 0.0, 3.0) = Complex{Float64}(1.0, 3.0)
     │
     ▼
  結果: 1.0 + 3.0im  ✓ (本家と同一)
```

## Rational: Pure Julia Inventory

### Source Files

| File | Lines | Contents |
|------|-------|----------|
| `julia/base/rational.jl` | 1,119 | Type, constructors, operators, math functions |
| `julia/base/int.jl` | 551 | `gcd`, `div` (Pure Julia, intrinsic wrappers) |
| `julia/base/intfuncs.jl` | 578 | `lcm`, `factorial`, BigInt integer helpers |
| `julia/base/promotion.jl` | — | `promote_rule`, `convert` for Rational |

### Pure Julia Functions (complete list)

| Category | Functions | Count |
|----------|-----------|-------|
| Type definition | `struct Rational{T<:Integer} <: Real` | 1 |
| Constructors | `Rational(num, den)` for Int64/32/16/8/BigInt + mixed-type | 8 |
| `//` operator | `//(n, d)` for Int64/32/16/8/BigInt + fallback | 6 |
| Accessors | `numerator`, `denominator` | 4 |
| Predicates | `iszero`, `isone`, `isinteger`, `signbit` | 4 |
| Unary | `-`, `inv` (+ BigInt specializations) | 4 |
| Arithmetic | `+`, `-`, `*`, `/` (+ BigInt specializations) | 8 |
| Comparison | `==`, `<`, `<=`, `>`, `>=` (+ BigInt cross-type) | 10+ |
| Math | `abs`, `sign`, `floor`, `ceil`, `round`, `^` | 6 |
| GCD/LCM | `gcd(::Rational)`, `lcm(::Rational)` | 2 |
| Rationalize | Stern-Brocot algorithm (7 methods) | 7 |
| Division | `div`, `fld`, `cld`, `rem`, `mod` (Rational & mixed) | 15 |
| Promotion | `promote_rule`, `convert` | 10+ |
| **Supporting** | `gcd(::Int64)` = Euclidean algorithm (**Pure Julia**) | 4 |

### Rust Rational Boundaries

| What | File | Why Not Pure Julia |
|------|------|--------------------|
| Struct/type recognition and field extraction | `vm/value/struct_instance.rs`, `vm/type_objects.rs` | VM carriers need to recognize `Rational{T}` values for display and conversion |
| Display `num//den` | `vm/formatting.rs` | VM-level display formatting |
| Numeric conversion | `vm/exec/conversion.rs`, `vm/type_ops/conversion.rs` | Dynamic conversion instructions and type constructors operate on VM `Value`s |
| Native-array conversion boundary | `vm/exec/array_basic.rs` | Native array conversion may need to preserve `Rational` struct values |

These are representation and conversion boundaries. Rational arithmetic itself
still goes through standard Julia multiple dispatch; the dynamic operator paths
explicitly route `Rational` values away from Rust arithmetic fast paths.

## Complex: Pure Julia Inventory

### Source Files

| File | Lines | Contents |
|------|-------|----------|
| `julia/base/complex.jl` | 889 | Type, constructors, operators, transcendentals |
| `julia/base/special/trig.jl` | 305 | `sin`, `cos`, `tan` for Complex (polynomial approximations) |
| `julia/base/special/exp.jl` | 91 | `exp` for Complex |
| `julia/base/special/log.jl` | 70 | `log` for Complex |
| `julia/base/promotion.jl` | — | `promote_rule`, `convert` for Complex |

### Pure Julia Functions (~120 total)

| Category | Functions | Count |
|----------|-----------|-------|
| Type definition | `struct Complex{T<:Real} <: Number`, aliases | 3 |
| Constructors | Two-arg, single-arg, parametric, mixed-type | 20+ |
| Constant | `const im = Complex{Bool}(false, true)` | 1 |
| Accessors | `real`, `imag` (Complex + Real fallbacks) | 4 |
| Predicates | `iszero`, `isreal`, `isfinite`, `isnan`, `isinf` | 5 |
| Unary | `-`, `conj`, `adjoint`, `transpose` | 6 |
| Arithmetic | `+`, `-`, `*`, `/` (Complex-Complex & Real-Complex) | 12 |
| Comparison | `==`, `!=` (15 cross-type specializations) | 15 |
| Identity | `zero`, `one` (instance & type, all T variants) | 16 |
| Math | `abs`, `abs2`, `angle` | 3 |
| Transcendental | `exp`, `log`, `sqrt` | 3 |
| Trigonometric | `sin`, `cos`, `tan` (Complex-specific, polynomial) | 3 |
| Special | `cis`, `cispi`, `reim` | 6 |
| Power | `^` (Complex^Complex, Complex^Real, Real^Complex) | 3 |
| Conversion | `float`, `conj!` (array) | 2 |
| Promotion | `promote_rule`, `convert` | 15+ |

### Rust Special-Cases (~40 sites)

#### VM Value Layer (detection & extraction)

| What | File | Purpose |
|------|------|---------|
| `is_complex()` | `struct_instance.rs`, `value_enum.rs` | Struct name pattern match |
| `as_complex_parts()` | `struct_instance.rs`, `value_enum.rs` | Extract `(re, im)` tuple |
| `complex_struct()` | `value_enum.rs` | Factory for Complex Value |
| `format_complex_struct()` | `formatting.rs` | Display as `re + im*im` |

#### Array & Broadcast (performance-critical)

| What | File | Purpose |
|------|------|---------|
| `complex_add/sub/mul/div` | `broadcast.rs` | Inline `(f64,f64)` arithmetic |
| `complex_pow/exp/log` | `broadcast.rs` | Array-level transcendentals |
| `broadcast_op_complex()` | `broadcast.rs` | Interleaved array broadcast |
| `is_complex_array()` | `matmul.rs` | Detect Complex matrices |
| `extract_complex_data()` | `matmul.rs` | Matrix data extraction |
| `matmul_complex()` | `matmul.rs` | Complex matrix multiplication |
| Interleaved storage | `array_value.rs` | `[re0, im0, re1, im1, ...]` format |

#### Compiler Type Inference

| What | File | Purpose |
|------|------|---------|
| `tfunc_real/imag` | `tfuncs/complex_ops.rs` | `Complex{T}` → `T` inference |
| `tfunc_conj` | `tfuncs/complex_ops.rs` | `Complex{T}` → `Complex{T}` |
| `tfunc_abs2/angle/reim` | `tfuncs/complex_ops.rs` | Return type inference |
| Complex dispatch routing | `compile/expr/binary/` | Route to Julia dispatch |
| `im` constant lowering | `lowering/expr/mod.rs` | Hardcoded struct literal |

## Why Complex Has More Rust Than Rational

```
                    Rational              Complex
                    ────────              ───────
Scalar arithmetic   Julia dispatch        Julia dispatch       ← 同じ
Array operations    ほぼ使わない           interleaved 配列     ← 差の原因
Matrix operations   非対応                matmul_complex
Broadcasting        使わない              complex_add 等
Type inference      不要                  tfunc 7関数
Display             num//den (1箇所)      re + im*im (1箇所)
```

**Scalar operations are identical in design** — both delegate entirely to Julia dispatch. The gap comes from **array/matrix/broadcast operations**, where Complex arrays use interleaved storage (`[re0, im0, re1, im1, ...]`) requiring dedicated Rust indexing, broadcasting, and matrix multiplication code.

## Rust Intrinsics Dependency

These are CPU-level operations that **cannot** be written in Pure Julia. They are not specific to Rational or Complex — all numeric code depends on them.

| Intrinsic | What | Rational | Complex |
|-----------|------|----------|---------|
| `add_int` / `sub_int` / `mul_int` | Integer arithmetic | Yes | Yes (mixed ops) |
| `sdiv_int` / `srem_int` | Integer division/remainder | Yes | No |
| `add_float` / `sub_float` / `mul_float` / `div_float` | Float arithmetic | No | Yes |
| `sqrt_llvm` | Square root (FPU) | No | Yes |
| `eq_int` / `slt_int` / `sle_int` / `sgt_int` / `sge_int` | Integer comparison | Yes | No |
| `eq_float` / `lt_float` / `le_float` / `gt_float` / `ge_float` | Float comparison | No | Yes |
| BigInt ops (`add_bigint`, etc.) | GMP library | Yes | No |

### What Is NOT an Intrinsic

| Function | Location | Implementation |
|----------|----------|----------------|
| `gcd(::Int64)` | `julia/base/int.jl` | **Pure Julia** (Euclidean algorithm using `%` and `abs`) |
| `lcm` | `julia/base/intfuncs.jl` | **Pure Julia** |
| `factorial` | `julia/base/intfuncs.jl` | **Pure Julia** |

Comment in `vm/builtins_math.rs:395`:
```rust
// Note: gcd, lcm, factorial removed - now Pure Julia (base/intfuncs.jl)
```

## VM Dispatch Pattern (shared by Rational & Complex)

Both types follow the same VM dispatch pattern:

```rust
// vm/dynamic_ops/mod.rs
fn should_use_inline_dynamic_op(&self, a: &Value, b: &Value) -> bool {
    // I64+I64, F64+F64, etc. → true (Rust fast path)
    // Array, BigInt → true (dedicated handlers)
    // Rational, Complex → false → Julia dispatch
}
```

```rust
// vm/exec/arithmetic.rs — DynamicAdd handler
let b = self.stack.pop_value()?;
let a = self.stack.pop_value()?;
if self.should_use_inline_dynamic_op(&a, &b) {
    self.stack.push(self.dynamic_add(&a, &b)?);
} else {
    // Rational & Complex: always this path
    if let Some(func_index) = self.find_best_method_index(&["+"], &values) {
        self.start_function_call(func_index, values)?;  // → Julia code
    }
}
```

## Comparison with Julia Official Implementation

### Rational

| Aspect | Julia Official | SubsetJuliaVM |
|--------|---------------|---------------|
| `//` parsing | `prec-rational` in Scheme parser | `Token::SlashSlash`, `Precedence::Rational=23` |
| `//` semantics | `Expr::Call` (regular function) | `Expr::Call` (same) |
| `+(::Integer, ::Rational)` | `@eval`-generated specialized method (direct) | `promote` → same-type `+(::Rational, ::Rational)` |
| GCD normalization | `divgcd` + `checked_den` | Constructor-internal `gcd` + `div` |
| Result | `5//2` | `5//2` (identical) |

### Complex

| Aspect | Julia Official | SubsetJuliaVM |
|--------|---------------|---------------|
| `im` definition | Runtime global `const im = Complex(false,true)` | Lowering-time hardcoded `Literal::Struct` |
| Juxtaposition | `juxtapose?` in Scheme parser | `JuxtapositionExpression` CST node |
| `+(::Real, ::Complex)` | Specialized method (direct) | `CallDynamicBinaryBoth` → Julia dispatch |
| Complex representation | Primitive struct (JIT-optimized) | `Value::Struct(StructInstance)` (heap) |
| Array storage | Native SIMD | Interleaved `[re0, im0, ...]` in Rust |
| Result | `1.0 + 3.0im` | `1.0 + 3.0im` (identical) |

## Key Differences Between `//` and `im` Lowering

| | `//` | `im` |
|---|---|---|
| Lowering result | `Expr::Call { function: "//" }` | `Literal::Struct("Complex{Bool}", ...)` |
| Resolution time | Runtime (function dispatch) | Lowering time (constant folding) |
| Why | `//` is a user-definable function | `im` is an immutable constant |
| User override | Possible (define new `//` method) | Not possible (hardcoded) |

## Audit Commands

### Verify No Rust Intrinsics for Rational Arithmetic

```bash
# Should print nothing: Rational arithmetic must not live in builtins_math.rs.
rg -n "Rational" subset_julia_vm_vm/src/vm/builtins_math.rs

# Representation/conversion boundaries are allowed; investigate any new
# arithmetic handler that does more than route Rational values to Julia dispatch.
rg -n 'is_rational|as_rational_parts|RATIONAL_STRUCT_NAME' \
  subset_julia_vm_vm/src/vm/value/struct_instance.rs \
  subset_julia_vm_vm/src/vm/exec/conversion.rs \
  subset_julia_vm_vm/src/vm/type_ops/conversion.rs \
  subset_julia_vm_vm/src/vm/formatting.rs
rg -n 'Rational arithmetic|Complex and Rational arithmetic' \
  subset_julia_vm_vm/src/vm/dynamic_ops \
  subset_julia_vm_vm/src/vm/exec/arithmetic.rs
```

### Verify gcd Is Pure Julia

```bash
# Should show Pure Julia implementation
rg -n -A 10 'function gcd\(a::Int64' subset_julia_vm/src/julia/base/int.jl

# Should confirm removal from Rust builtins
rg -n 'gcd, lcm, factorial removed' subset_julia_vm_vm/src/vm/builtins_math.rs
```

### Count Pure Julia vs Rust for Complex

```bash
# Pure Julia functions
rg -c '^(function|Base\.:)' subset_julia_vm/src/julia/base/complex.jl

# Rust special-cases
rg -n 'is_complex|as_complex_parts|complex_struct|Complex' \
  subset_julia_vm_vm/src/vm -g '*.rs' -g '!**/*test*' | rg -v '// ' | wc -l
```

## Keyword Arguments in Base-Loaded Functions (Issue #2624)

### Known Limitation

Functions defined in Base-loaded `.jl` files that use keyword arguments may not dispatch correctly when called **without** kwargs. This is because the compiler generates separate method entries for the kwargs variant and the positional-only variant, and the dispatch mechanism may not find the positional-only fallback.

```julia
# In a Base-loaded .jl file:
function range(start; length=nothing, stop=nothing, step=nothing)
    # ...
end

# This works:
range(1; length=5)  # kwargs provided → dispatches correctly

# This may fail:
range(1, 10)  # no kwargs → dispatch may not find the method
```

### Workarounds

1. **Use positional arguments** where possible instead of kwargs
2. **Provide explicit wrapper methods** with positional arguments that call the kwargs version:
   ```julia
   range(start, stop) = range(start; stop=stop)
   range(start, stop, step) = range(start; stop=stop, step=step)
   ```
3. **Test both calling conventions** (with and without kwargs) in fixture tests

### Impact on Pure Julia Migration

When migrating Rust builtins to Pure Julia, prefer **positional-argument APIs** over kwargs-based APIs. If the official Julia API uses kwargs, provide both:
- A kwargs method matching the official API
- A positional-argument wrapper for the common case

## New `.jl` File Checklist (Issue #2624)

When adding a new Pure Julia source file to the project, three locations in `mod.rs` must be updated:

### 1. Declare the constant (`subset_julia_vm/src/julia/base/mod.rs`)

```rust
/// Description of what this file implements
/// Based on Julia's base/yourfile.jl
pub const YOURFILE_JL: &str = include_str!("yourfile.jl");
```

### 2. Add to `get_base()` concatenation (`subset_julia_vm/src/julia/base/mod.rs`)

```rust
pub fn get_base() -> String {
    format!(
        "...{}\n{}",
        // ... existing files ...
        EXISTING_JL,      // N. Previous file
        YOURFILE_JL,      // N+1. Your new file
    )
}
```

**Order matters!** Files are compiled in concatenation order. If your file depends on types/functions from another file, it must come after that file.

### 3. Verify loading

```bash
# Build to verify the file is included without errors
cargo build 2>&1 | head -20

# Run tests to verify functions are accessible
timeout 1800 cargo nextest run --release --test fixture_tests <category>::
```

### Common Mistakes

- **Missing from `get_base()`**: File exists but functions are never loaded → `NoMethodFound` errors
- **Wrong order in `get_base()`**: File loaded before its dependencies → compile errors in the Julia code
- **Missing `include_str!`**: Rust compile error (file not embedded in binary)

## Rust Builtin → Pure Julia Migration Checklist Template (Issue #2624)

Use this template when migrating any Rust builtin to Pure Julia:

```markdown
### Migration: `function_name`

- [ ] **Reference**: Read official Julia implementation in `julia/base/`
- [ ] **Implement**: Create Pure Julia in `subset_julia_vm/src/julia/base/<file>.jl`
- [ ] **Load**: Add to `mod.rs` (constant + `get_base()`) — see New .jl File Checklist
- [ ] **Export**: Add to `exports.jl` if user-facing
- [ ] **Test**: Add fixture test in `subset_julia_vm/tests/fixtures/<category>/`
- [ ] **Verify**: Run `julia test.jl` to confirm behavior matches official Julia
- [ ] **Check kwargs**: If function uses kwargs, test both with and without kwargs
- [ ] **Check multi-type**: If builtin handles multiple types, migrate ALL types (Issue #2711)
- [ ] **Check mutation**: If function mutates, keep as Rust builtin (Issue #2709)
- [ ] **Remove Rust**: Follow BUILTIN_REMOVAL.md checklist to remove all Rust traces
- [ ] **Build**: `cargo build` succeeds
- [ ] **Test**: `timeout 1800 cargo nextest run --release --test fixture_tests <category>::` passes
```

## Migration Guidance: Always Compare with Official Julia (Issue #2612)

When migrating a function from Rust builtin to Pure Julia, the **source of truth is official
Julia**, not the existing Rust implementation. Rust builtins may have subtly different behavior
(see `docs/vm/BUILTIN_REMOVAL.md` "Compatibility Audit" section).

### Migration Verification Workflow

1. **Before writing Pure Julia code**, test the function in official Julia:
   ```bash
   julia -e '
     # Test normal cases
     println(titlecase("hello world"))
     # Test edge cases
     println(titlecase(""))
     println(titlecase("HELLO"))
     println(titlecase("hello-world"))
   '
   ```

2. **Write the Pure Julia implementation** to match official Julia's behavior exactly.

3. **Run the same tests in SubsetJuliaVM** and compare:
   ```bash
   cargo run --features repl -- -e '
     println(titlecase("hello world"))
     println(titlecase(""))
     println(titlecase("HELLO"))
     println(titlecase("hello-world"))
   '
   ```

4. **Add fixture tests** that assert the official Julia behavior:
   ```julia
   using Test
   @testset "titlecase edge cases" begin
       @test titlecase("hello world") == "Hello World"
       @test titlecase("") == ""
       @test titlecase("HELLO") == "Hello"
       # Verify these match `julia -e 'println(titlecase("hello-world"))'`
       @test titlecase("hello-world") == "Hello-World"
   end
   true
   ```

### Common Mistake: Copying Rust Behavior

```julia
# BAD: Copied from Rust implementation which may differ from Julia
function titlecase(s::AbstractString)
    # ... Rust-derived logic that capitalizes first letter only ...
end

# GOOD: Verified against julia -e 'titlecase("hello world")'
# Julia titlecases after every non-letter character
function titlecase(s::AbstractString)
    # ... logic matching official Julia behavior exactly ...
end
```

### Edge Case Verification Checklist

When migrating any function, always verify these edge cases against official Julia:

- [ ] Empty input (`""`, `[]`, `nothing`)
- [ ] Single element input
- [ ] Mixed types (e.g., `Int64` + `Float64`)
- [ ] Boundary values (`typemin`, `typemax`, `Inf`, `NaN`)
- [ ] Error conditions (what errors does Julia throw?)

## Trigonometric/Exponential/Logarithmic Functions: Pure Julia (Phase 6)

Implemented in Phase 6-1 and 6-2, these functions are now **fully Pure Julia** for `Float64`:

### Source Files

| File | Lines | Contents |
|------|-------|----------|
| `julia/base/special/trig.jl` | 305 | `sin(::Float64)`, `cos(::Float64)`, `tan(::Float64)` using polynomial approximations |
| `julia/base/special/exp.jl` | 91 | `exp(::Float64)` using polynomial approximation |
| `julia/base/special/log.jl` | 70 | `log(::Float64)` using polynomial approximation |
| `julia/base/math.jl` | 586 | Wrappers: `sinpi`, `cospi`, `sinc`, `cosc`, `sincos`, degree variants, `log2`, `log10`, `log1p`, `exp2`, `exp10`, `expm1`, hyperbolic functions (`sinh`, `cosh`, `tanh`, `asinh`, `acosh`, `atanh`) |

### Key Design: Polynomial Approximation in Pure Julia

The `sin`, `cos`, `tan`, `exp`, and `log` functions for `Float64` use polynomial approximations (Chebyshev or minimax) implemented entirely in Julia. No Rust intrinsics are needed beyond basic float arithmetic (`add_float`, `mul_float`, `div_float`).

### Function Count: ~22 (in special/) + ~55 (in math.jl)

## Dict{K,V}: Pure Julia Hash Table (Milestone 9; carrier retired #6731)

Dict is now a **single Pure Julia implementation**: the `Dict{K,V}` struct with
a full open-addressing hash table. The legacy Rust-backed `Value::Dict` carrier
that previously coexisted with the struct was **fully retired in Issue #6731** —
`Dict()` / `Dict("a" => 1)` constructor calls now build the Pure Julia struct,
and public `Dict` API resolves through ordinary method dispatch. The only
remaining `Value::Dict` references in Rust are unreachable error strings (e.g.
`vm/exec/dict.rs`: "DictSet is unreachable after Value::Dict removal") and
removal-documenting comments.

### Source Files

| File | Lines | Contents |
|------|-------|----------|
| `julia/base/dict.jl` | 622 | Hash table struct, algorithms, and public API (~36 functions) |

### Pure Julia Functions

| Category | Functions | Count |
|----------|-----------|-------|
| Hash table internals | `_tablesz`, `_shorthash7`, `hashindex`, `ht_keyindex`, `ht_keyindex2!`, `_setindex!`, `_delete!`, `rehash!`, `skip_deleted`, `skip_deleted_floor!`, `_new_dict_kv` | 11 |
| Struct definition | `mutable struct Dict{K,V} <: AbstractDict{K,V}` | 1 |
| Read operations | `getindex`, `haskey`, `get`, `length`, `isempty` | 5 |
| Write operations | `setindex!`, `delete!`, `empty!`, `pop!` (2 variants) | 5 |
| Iteration | `iterate` (2 variants), `keys`, `values`, `pairs` | 5 |
| Public algebra | `merge`, `mergewith!`, `mergewith`, `getkey`, `get!`, `filter`, `map` over pairs | 9 |

### Construction Pattern (post-#6731)

`Dict()` / `Dict{K,V}()` with empty or pair arguments now construct the Pure
Julia struct directly. Both bare `::Dict` and parametric `::Dict{K,V} where
{K,V}` method annotations dispatch on that single struct representation; there
is no longer a Rust-carrier branch to disambiguate.

## Memory{T} and Array{T}: Pure Julia Foundation

### Memory{T} (Rust Primitive + Pure Julia Shape Protocol)

`Memory{T}` is a native Rust primitive type representing a fixed-size typed buffer, mirroring Julia 1.11+'s `GenericMemory`. It serves as the foundation for the Pure Julia `Array{T}` wrapper.

| File | Lines | Contents |
|------|-------|----------|
| `julia/base/genericmemory.jl` | 195 | `size`, `ndims`, `eltype`, `keys`, `values`, `similar`, `copy`, `parent`, `memoryindex` |

Constructor `Memory{T}(n)`, low-level `length`, `getindex`, `setindex!`, and the underlying allocation are handled natively in Rust; the shape protocol and higher-level helpers above are Pure Julia.

### Array{T} (Pure Julia Wrapper)

`Array{T}` is a Pure Julia mutable struct that wraps a `Memory{T}` (or a `MemoryRef{T}` offset into one) together with dimension metadata:

```julia
mutable struct Array{T}
    _mem    # Memory{T} backing storage (or MemoryRef-backed via offset-encoded _size)
    _size   # Dimension tuple, or `(dims, offset)` for MemoryRef-backed wrappers
end
```

| File | Lines | Public coverage |
|------|-------|-----------------|
| `julia/base/array.jl` | 4,303 | `wrap`, `size`, `length`, `ndims`, `eltype`, `getindex` / `setindex!` (1D–ND, ranges, masks, colons), `reshape`, `similar` (typed + untyped), mutation (`push!`, `pushfirst!`, `pop!`, `popfirst!`, `insert!`, `deleteat!`, `append!`, `prepend!`), `copy`, `copyto!`, `fill`, `fill!`, `findall`/`findfirst`/`findlast`, `axes`, `firstindex`/`lastindex`, `hcat`/`vcat`/`cat`, `permutedims`, `transpose`/`adjoint`, `map!`, `circshift`/`circshift!`, `rot180`/`rotl90`/`rotr90` |
| `julia/base/arraymath.jl` | 19 | Pure Julia arithmetic helpers used by Array wrappers |

Public `Base` array behavior dispatches through Pure Julia methods on the `Array{T}` wrapper. The old `Value::Array(ArrayRef)` carrier has been retired (Issue #4568); `scripts/check_value_array_allowlist.sh` is now a zero-match audit, and remaining native-array compatibility uses explicit `Value::NativeArray` converters. After Issue #6653, public construction/materialization/HOF/broadcast routes return MemoryRef-backed wrappers; retained `NativeArray` handlers are cache/VM/host compatibility fallbacks. New internal native-array allocations still use Memory-first `ArrayValue::memory_first_undef` / `memory_first_filled`, and the `memory_to_array_ref` compatibility bridge has been retired (`scripts/check_memory_to_array_ref_allowlist.sh`).

See `docs/vm/ARRAY_MEMORY_MIGRATION.md` for the current Array/Memory migration
status and the archived historical log link.

## Set: Pure Julia `Dict{T,Nothing}` Wrapper (carrier retired #6721/#6732)

`Set{T}` is now a **fully Pure Julia** struct that layers directly on top of the
Pure Julia `Dict{T,Nothing}` hash table, exactly as upstream Julia does. The
legacy Rust-backed `Value::Set` carrier and its `_set_push!` / `_set_delete!` /
`_set_in` / `_set_empty!` / `_set_length` intrinsics were **fully retired in
Issues #6721/#6732**. There are now **0** real `Value::Set` handler sites in
Rust — the remaining grep hits are removal-documenting comments only.

```julia
struct Set{T} <: AbstractSet{T}
    dict::Dict{T,Nothing}   # backing store; loaded after dict.jl in get_base()
end
```

| File | Lines | Contents |
|------|-------|----------|
| `julia/base/set.jl` | 674 | `Set{T}` struct + all operations delegated to the backing `Dict{T,Nothing}` |

### Pure Julia (~23 public functions)

Core wrappers (`push!`, `delete!`, `empty!`, `length`, `in`, `iterate`) delegate
to the backing `Dict{T,Nothing}`; set algebra (`union`, `intersect`, `setdiff`,
`symdiff`, `issubset`, `isdisjoint`, `issetequal`) and mutating variants
(`union!`, `intersect!`, `setdiff!`, `symdiff!`) are Pure Julia on top of those
primitives. No Rust `HashSet` is involved.

## Broadcast: Pure Julia Infrastructure

The broadcast system is one of the largest Pure Julia subsystems (~2,856 lines), implementing Julia's dot-syntax fusion:

| File | Lines | Contents |
|------|-------|----------|
| `julia/base/broadcast.jl` | 2,856 | BroadcastStyle, Broadcasted, materialize, copyto!, shape computation |
| `julia/stdlib/Broadcast/src/Broadcast.jl` | 55 | Additional broadcast types |

Key Pure Julia components: `BroadcastStyle` type hierarchy, `Broadcasted` lazy container, `materialize`/`materialize!` entry points, `copyto!` loop fusion, shape computation (`broadcast_shape`), and `.&&`/`.||` operators.

Rust fast paths exist for performance-critical array operations (e.g., `f64` broadcast, Complex array broadcast).

## Related Documentation

- `docs/vm/COMPLEX.md` — Complex operation support status and workarounds
- `docs/vm/TYPE_SYSTEM.md` — Type representations (LatticeType, ValueType, JuliaType)
- `docs/vm/BINARY_DISPATCH.md` — Binary operator dispatch paths
- `docs/vm/TYPE_PRESERVATION.md` — Float type preservation across three layers
- `docs/vm/NUMERIC_TYPES.md` — Numeric type parity checklist
- `docs/vm/BUILTIN_REMOVAL.md` — Builtin removal checklist, compatibility audit, dead code detection
