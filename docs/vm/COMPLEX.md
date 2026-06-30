# Complex Number Support in SubsetJuliaVM

This document describes the Complex number implementation in SubsetJuliaVM, covering supported operations, the Pure Julia / Rust boundary, and implementation architecture.

For the Rational & Complex Pure Julia design comparison, see `docs/vm/PURE_JULIA_DESIGN.md`.

## Overview

SubsetJuliaVM supports complex numbers through the `Complex{T}` type, compatible with Julia's standard implementation. The domain logic (111 functions) is written in **Pure Julia** (`julia/base/complex.jl`), with Rust providing infrastructure for array operations, type inference, and display.

## Supported Operations

### Construction

| Operation | Example | Status |
|-----------|---------|--------|
| Constructor with two arguments | `Complex(1.0, 2.0)` | Supported |
| Constructor from real | `Complex(3.0)` | Supported |
| Imaginary unit | `im` | Supported |
| Literal syntax (juxtaposition) | `1.0 + 2.0im` | Supported |
| Type-parameterized | `Complex{Float64}(1, 2)` | Supported |
| Type aliases | `ComplexF64`, `ComplexF32` | Supported |
| Bool constructor | `Complex(true, false)` | Supported |
| Float32 constructor | `Complex{Float32}(1.0f0, 2.0f0)` | Supported |
| Mixed-type promotion | `Complex(1, 2.5)` → `Complex{Float64}` | Supported |
| Arbitrary `T<:Real` element | `Complex(Int32(1), Int32(2))` → `Complex{Int32}`, `Complex(1//2, 3//4)` → `Complex{Rational{Int64}}` | Supported (Issue #5131) |
| Single-arg arbitrary `T<:Real` | `Complex(Int32(5))` → `Complex{Int32}` | Supported (Issue #5131) |

> **Parametric completeness (Issue #5131):** the generic constructors
> `Complex(x::Real, y::Real)` and `Complex(x::Real)` preserve and infer the
> element type `T` for *any* `T<:Real` — not only the element types with an
> explicit concrete overload. They promote first and then build the parametric
> inner constructor `Complex{typeof(...)}(...)` directly, instead of re-dispatching
> through `Complex(promote(x,y)...)`. The re-dispatch form recursed infinitely in
> SubsetJuliaVM because its method specificity ranks an abstract `::Real`
> parameter above a same-binding `where {T<:Real}` type variable, so the splat
> resolved back to `(::Real, ::Real)`.

### Scalar Arithmetic

| Operation | Example | Status | Implementation |
|-----------|---------|--------|----------------|
| Addition | `c1 + c2` | Supported | Pure Julia |
| Subtraction | `c1 - c2` | Supported | Pure Julia |
| Multiplication | `c1 * c2` | Supported | Pure Julia |
| Division | `c1 / c2` | Supported | Pure Julia |
| Power | `c1 ^ c2`, `c1 ^ 2` | Supported | Pure Julia |
| Unary negation | `-c1` | Supported | Pure Julia |
| Real + Complex | `1 + c`, `c + 1` | Supported | Pure Julia |
| Real * Complex | `2.0 * c`, `c * 2.0` | Supported | Pure Julia |
| Real / Complex | `1.0 / c`, `c / 2.0` | Supported | Pure Julia |

### Scalar Functions

| Function | Example | Status | Implementation |
|----------|---------|--------|----------------|
| `real(z)` | Get real part | Supported | Pure Julia |
| `imag(z)` | Get imaginary part | Supported | Pure Julia |
| `conj(z)` | Complex conjugate | Supported | Pure Julia |
| `adjoint(z)` / `z'` | Adjoint (= conj for scalars) | Supported | Pure Julia |
| `transpose(z)` | Transpose (identity for scalars) | Supported | Pure Julia |
| `abs(z)` | Magnitude | Supported | Pure Julia |
| `abs2(z)` | Squared magnitude | Supported | Pure Julia |
| `angle(z)` | Phase angle | Supported | Pure Julia |
| `sqrt(z)` | Square root | Supported | Pure Julia |
| `exp(z)` | Exponential | Supported | Pure Julia |
| `log(z)` | Natural logarithm | Supported | Pure Julia |
| `sin(z)`, `cos(z)`, `tan(z)` | Trigonometric | Supported | Pure Julia |
| `cis(x)`, `cispi(x)` | `cos(x) + i*sin(x)` | Supported | Pure Julia |
| `reim(z)` | Returns `(re, im)` tuple | Supported | Pure Julia |
| `float(z)` | Convert to `Complex{Float64}` | Supported | Pure Julia |
| `conj!(A)` | In-place conjugate for arrays | Supported | Pure Julia |

### Predicates

| Predicate | Example | Status | Implementation |
|-----------|---------|--------|----------------|
| `iszero(z)` | Both parts zero | Supported | Pure Julia |
| `isreal(z)` | Imaginary part is zero | Supported | Pure Julia |
| `isfinite(z)` | Both parts finite | Supported | Pure Julia |
| `isnan(z)` | Either part NaN | Supported | Pure Julia |
| `isinf(z)` | Either part Inf | Supported | Pure Julia |

### Comparison

| Operation | Example | Status | Implementation |
|-----------|---------|--------|----------------|
| Equality | `c1 == c2` | Supported | Pure Julia (15 cross-type specializations) |
| Inequality | `c1 != c2` | Supported | Pure Julia |
| `===` (egal) | `c1 === c2` | Supported | VM |
| `isequal` | `isequal(c1, c2)` | Supported | Pure Julia |
| `hash` | `hash(c)` | Supported | VM |
| `isapprox` | `isapprox(c1, c2; atol=1e-6)` | Supported | Pure Julia |

### Array Operations

| Operation | Example | Status | Implementation |
|-----------|---------|--------|----------------|
| Complex scalar * Vector | `c * [1,2,3]` | Supported | Rust (broadcast) |
| Vector * Complex scalar | `[1,2,3] * c` | Supported | Rust (broadcast) |
| `im .* array` | `im .* [1.0, 2.0]` | Supported | Rust (broadcast, Issue #1904) |
| `array .+ im` | `[1.0, 2.0] .+ im` | Supported | Rust (broadcast) |
| Matrix multiplication | `A * B` (complex) | Supported | Rust (`matmul_complex`) |
| `isapprox` on arrays | `isapprox(a, b)` | Supported | Pure Julia |

### Type Promotion

Complex type promotion follows Julia's rules (all Pure Julia in `promotion.jl`):

| Operation | Result Type |
|-----------|-------------|
| `Complex{Bool} + Int64` | `Complex{Int64}` |
| `Complex{Bool} + Float64` | `Complex{Float64}` |
| `Complex{Int64} + Float64` | `Complex{Float64}` |
| `Complex{Float32} + Complex{Float64}` | `Complex{Float64}` |
| `Complex{Float32} + Float64` | `Complex{Float64}` |
| `Complex{Float64} + Int64` | `Complex{Float64}` |

## Implementation Architecture

### Three-Layer Design

```
Layer 3: Pure Julia (complex.jl, promotion.jl)
  ├── struct Complex{T<:Real} <: Number
  ├── const im = Complex{Bool}(false, true)
  ├── Arithmetic: +, -, *, / (12 methods)
  ├── Comparison: ==, != (15 cross-type methods)
  ├── Transcendental: exp, log, sqrt, sin, cos, tan
  ├── Constructors: 20+ methods
  ├── Display: Base.show(io, ::Complex) — string/print/repr/show (Issue #5155)
  └── Promotion: promote_rule, convert
      │
Layer 2: Rust Infrastructure
  ├── Value representation: Value::Struct(StructInstance)
  ├── Detection: is_complex(), as_complex_parts()
  ├── Display fallback: format_complex_struct() — non-VM paths only,
  │     kept consistent with the pure-Julia show (Issue #5155)
  ├── Type inference: tfunc_real/imag/conj/abs2/angle/reim
  ├── Dispatch routing: CallDynamicBinaryBoth for Complex
  ├── Array broadcast: broadcast_op_complex(), complex_add/sub/mul/div
  └── Matrix: matmul_complex(), extract_complex_data()
      │
Layer 1: Rust Intrinsics (CPU instructions)
  ├── add_float, sub_float, mul_float, div_float
  ├── sqrt_llvm
  └── eq_float, lt_float, ...
```

### Scalar Dispatch Path

For `1 + 3.0im`:

```
1 + 3.0im
    │
    ▼  Parser: JuxtapositionExpression(3.0, im)
    ▼  Lowering: BinaryOp::Mul(3.0, Complex{Bool}(false,true))
    ▼  Compile: CallDynamicBinaryBoth (Complex detected)
    │
    ▼  VM: should_use_inline_dynamic_op(I64, Struct) → false
    ▼  VM: find_best_method_index(&["+"], &[I64, Complex{F64}])
    ▼  → Julia dispatch to +(x::Real, z::Complex)
    │
    ▼  Pure Julia: Complex(x + real(z), imag(z))
    ▼  = Complex{Float64}(1.0, 3.0)
```

### Array Dispatch Path

For `im .* [1.0, 2.0, 3.0]`:

```
im .* [1.0, 2.0, 3.0]
    │
    ▼  Broadcast infrastructure detects Complex operand
    ▼  broadcast.rs: Broadcastable::is_complex() → true
    ▼  → broadcast_op_complex() (Rust fast path)
    │
    ▼  Rust inline: complex_mul((0.0, 1.0), (x, 0.0)) for each element
    ▼  Result: interleaved [0.0, 1.0, 0.0, 2.0, 0.0, 3.0]
```

### Interleaved Array Storage

Complex arrays use interleaved storage for cache efficiency:

```
Logical:  [Complex(1,2), Complex(3,4), Complex(5,6)]
Physical: [1.0, 2.0, 3.0, 4.0, 5.0, 6.0]  (interleaved re, im pairs)
          element_type_override = ComplexF64
```

### Key Files

| File | Purpose | Layer |
|------|---------|-------|
| `julia/base/complex.jl` | Type, operators, transcendentals (111 functions) | Pure Julia |
| `julia/base/promotion.jl` | `promote_rule`, `convert` for Complex | Pure Julia |
| `vm/value/struct_instance.rs` | `is_complex()`, `as_complex_parts()` | Rust |
| `vm/formatting.rs` | Display as `re + im*im` | Rust |
| `vm/broadcast.rs` | `complex_add/sub/mul/div`, `broadcast_op_complex` | Rust |
| `vm/matmul/` | `matmul_complex` (`multiply.rs`), `extract_complex_data` (`helpers.rs`) | Rust |
| `vm/dynamic_ops/` | `should_use_inline_dynamic_op` (`dispatch.rs`, returns false for Complex) | Rust |
| `compile/tfuncs/complex_ops.rs` | `tfunc_real/imag/conj/abs2/angle/reim` | Rust |
| `compile/expr/binary/` | Complex dispatch routing | Rust |
| `lowering/expr/mod.rs` | `im` constant hardcoded as struct literal | Rust |

## Testing

### Running Complex Tests

```bash
# All complex tests
timeout 1800 cargo nextest run --release --test fixture_tests complex::

# Specific test
timeout 1800 cargo nextest run --release --test fixture_tests complex_im_literal_operations

# Mixed-type tests (Int + Complex, etc.)
timeout 1800 cargo nextest run --release --test fixture_tests mixed::
```

### Test Fixtures (28 tests in `tests/fixtures/complex/`)

| Category | Tests |
|----------|-------|
| Construction | `parametric_float64/int64/float32`, `inference_int`, `widening` |
| `im` literal | `im_literal_operations` (Issue #920), `im_broadcast_scalar` (Issue #1904) |
| Arithmetic | `complex_math` (abs, exp, sqrt, log), `complex_trig` (sin, cos, tan) |
| Dispatch | `primitive_dispatch` (Issue #2235), `generic_dispatch` |
| Arrays | `array_broadcast`, `array_interleaved`, `scalar_complex_array_mul`, `scalar_real_array_mul`, `mixed_real_complex_array_ops`, `mixed_array_promotion`, `undef_array_write` |
| Comparison | `complex_equality` (===, isequal, hash) |
| Float32 | `complexf32_support` (Issue #1399), `float32_field_access` |
| Display | `complex_display_formatting` |
| Predicates | `predicates` (iszero, isreal, isfinite, isnan, isinf) |
| Special | `cis_reim`, `bool_strong_zero`, `nested_expr_type`, `adjoint_double`, `wrapped_functions` |

## Related Documentation

- `docs/vm/PURE_JULIA_DESIGN.md` — Pure Julia design comparison (Rational vs Complex)
- `docs/vm/TYPE_SYSTEM.md` — Type representations (LatticeType, ValueType, JuliaType)
- `docs/vm/BINARY_DISPATCH.md` — Binary operator dispatch paths
- `docs/vm/TYPE_PRESERVATION.md` — Float type preservation
- `julia/base/complex.jl` — Reference Julia implementation
