# Scalar Math Functions

**Last Updated**: 2026-01-08 (all scalar math functions complete, exports updated)

Julia Base からエクスポートされているスカラー数学関数の実装状況。

## Summary

| Category | Total | Implemented | Unimplemented | Rate |
|----------|-------|-------------|---------------|------|
| Trigonometric | 31 | 31 | 0 | 100% |
| Hyperbolic | 12 | 12 | 0 | 100% |
| Exponential/Logarithmic | 8 | 8 | 0 | 100% |
| Power/Roots | 4 | 4 | 0 | 100% |
| Rounding | 4 | 4 | 0 | 100% |
| Float Properties | 12 | 12 | 0 | 100% |
| Sign/Absolute | 8 | 8 | 0 | 100% |
| Integer Arithmetic | 19 | 19 | 0 | 100% |
| Bit Operations | 9 | 9 | 0 | 100% |
| Comparison/Predicates | 14 | 14 | 0 | 100% |
| Type Conversion | 13 | 13 | 0 | 100% |
| Clamp/Range | 4 | 4 | 0 | 100% |
| Fused Operations | 2 | 2 | 0 | 100% |
| Other | 19 | 19 | 0 | 100% |
| **Total** | **159** | **159** | **0** | **100%** |

**Note**: `unsafe_trunc` is implemented for Int64/Int32/Int16/Int8. Generic version with `where` clause is now supported but explicit type methods are preferred for clarity.

---

## Implementation Details

### Trigonometric Functions (31/31 = 100%)

All trigonometric functions are implemented.

| Function | Implementation | Notes |
|----------|----------------|-------|
| `sin`, `cos`, `tan` | Rust Builtin | Basic trig |
| `asin`, `acos`, `atan` | Rust Builtin | Inverse trig |
| `sind`, `cosd`, `tand` | Pure Julia (`math.jl`) | Degree-based |
| `asind`, `acosd`, `atand` | Pure Julia (`math.jl`) | Inverse degree-based |
| `sinpi`, `cospi` | Pure Julia (`math.jl`) | `sin(π*x)`, `cos(π*x)` |
| `sinc`, `cosc` | Pure Julia (`math.jl`) | Normalized sinc |
| `sincos`, `sincosd`, `sincospi` | Pure Julia (`math.jl`) | Combined sin/cos |
| `sec`, `csc`, `cot` | Pure Julia (`math.jl`) | Reciprocal trig |
| `secd`, `cscd`, `cotd` | Pure Julia (`math.jl`) | Reciprocal degree-based |
| `asec`, `acsc`, `acot` | Pure Julia (`math.jl`) | Inverse reciprocal |
| `asecd`, `acscd`, `acotd` | Pure Julia (`math.jl`) | Inverse reciprocal degree |

### Hyperbolic Functions (12/12 = 100%)

| Function | Status | Implementation | Notes |
|----------|--------|----------------|-------|
| `sinh`, `cosh`, `tanh` | ✅ | Rust Builtin | Basic hyperbolic |
| `asinh`, `acosh`, `atanh` | ✅ | Rust Builtin | Inverse hyperbolic |
| `sech`, `csch`, `coth` | ✅ | Pure Julia (`math.jl`) | Reciprocal hyperbolic |
| `asech` | ✅ | Pure Julia (`math.jl`) | `acosh(1/x)` |
| `acsch` | ✅ | Pure Julia (`math.jl`) | `asinh(1/x)` |
| `acoth` | ✅ | Pure Julia (`math.jl`) | `atanh(1/x)` |

### Exponential and Logarithmic (8/8 = 100%)

| Function | Implementation | Notes |
|----------|----------------|-------|
| `exp`, `exp2`, `exp10` | Rust Builtin | Exponential functions |
| `expm1` | Rust Builtin | `exp(x) - 1` (accurate for small x) |
| `log`, `log2`, `log10` | Rust Builtin | Logarithmic functions |
| `log1p` | Rust Builtin | `log(1 + x)` (accurate for small x) |

### Power and Roots (4/4 = 100%)

| Function | Implementation | Notes |
|----------|----------------|-------|
| `sqrt` (√) | VM Intrinsic | Square root |
| `cbrt` (∛) | Rust Builtin | Cube root |
| `fourthroot` (∜) | Rust Builtin | Fourth root |
| `hypot` | Rust Builtin / Pure Julia | `sqrt(x^2 + y^2)` without overflow |

### Rounding (4/4 = 100%)

| Function | Implementation | Notes |
|----------|----------------|-------|
| `floor` | VM Intrinsic | Round down |
| `ceil` | VM Intrinsic | Round up |
| `round` | Rust Builtin | Round to nearest |
| `trunc` | Rust Builtin | Round toward zero |

### Float Properties (12/12 = 100%)

| Function | Implementation | Notes |
|----------|----------------|-------|
| `nextfloat` | Rust Builtin | Next representable float |
| `prevfloat` | Rust Builtin | Previous representable float |
| `eps` | Pure Julia | Machine epsilon |
| `floatmax` | Pure Julia | Maximum finite float |
| `floatmin` | Pure Julia | Minimum positive float |
| `modf` | Pure Julia | Split into fractional and integer parts |
| `ldexp` | Pure Julia | `x * 2^n` |
| `exponent` | Rust Builtin | Get exponent of float |
| `significand` | Rust Builtin | Get significand of float |
| `frexp` | Rust Builtin | Split into mantissa and exponent |
| `issubnormal` | Rust Builtin | Check if subnormal |
| `maxintfloat` | Rust Builtin | Max integer representable as float |

### Sign and Absolute Value (8/8 = 100%)

| Function | Implementation | Notes |
|----------|----------------|-------|
| `abs` | Pure Julia (`number.jl`, `int.jl`, `float.jl`) | Absolute value |
| `abs2` | Pure Julia (`number.jl`) | Squared absolute value |
| `sign` | Pure Julia (`math.jl`) | Sign of number (-1, 0, 1) |
| `signbit` | Pure Julia (`number.jl`, `int.jl`, `float.jl`) | Sign bit |
| `copysign` | Pure Julia (`math.jl`) | Copy sign from y to x |
| `flipsign` | Pure Julia (`number.jl`) | Flip sign based on y |
| `isnegative` | Pure Julia (`number.jl`) | Check if negative |
| `ispositive` | Pure Julia (`number.jl`) | Check if positive |

### Integer Arithmetic (19/19 = 100%)

| Function | Implementation | Notes |
|----------|----------------|-------|
| `div` | Pure Julia (`math.jl`, `int.jl`) | Integer division |
| `divrem` | Pure Julia (`math.jl`) | Division and remainder |
| `fld` | Pure Julia (`math.jl`) | Floor division |
| `fldmod` | Pure Julia (`math.jl`) | Floor division and modulo |
| `fld1`, `fldmod1` | Pure Julia (`math.jl`) | 1-based variants |
| `mod` | Pure Julia (`math.jl`) | Modulo (floor semantics) |
| `mod1` | Pure Julia (`math.jl`) | 1-based modulo |
| `mod2pi` | Pure Julia (`math.jl`) | Modulo 2π |
| `rem` | Pure Julia (`math.jl`) | Remainder (truncate semantics) |
| `rem2pi` | Pure Julia (`math.jl`) | Remainder modulo 2π |
| `cld` | Pure Julia | Ceiling division |
| `gcd` | Pure Julia (`intfuncs.jl`) | Greatest common divisor |
| `gcdx` | Pure Julia (`intfuncs.jl`) | Extended GCD |
| `lcm` | Pure Julia (`intfuncs.jl`) | Least common multiple |
| `factorial` | Pure Julia (`intfuncs.jl`) | Factorial |
| `binomial` | Pure Julia (`combinatorics.jl`) | Binomial coefficient |
| `powermod` | Pure Julia (`intfuncs.jl`) | Modular exponentiation |
| `invmod` | Pure Julia (`intfuncs.jl`) | Modular inverse |
| `isqrt` | Pure Julia (`intfuncs.jl`) | Integer square root |

### Bit Operations (9/9 = 100%)

| Function | Implementation | Notes |
|----------|----------------|-------|
| `count_ones` | Rust Builtin | Popcount |
| `count_zeros` | Rust Builtin | Count zero bits |
| `leading_zeros` | Rust Builtin | Leading zero bits |
| `leading_ones` | Rust Builtin | Leading one bits |
| `trailing_zeros` | Rust Builtin | Trailing zero bits |
| `trailing_ones` | Rust Builtin | Trailing one bits |
| `bitreverse` | Rust Builtin | Reverse all bits |
| `bitrotate` | Rust Builtin | Rotate bits |
| `bswap` | Rust Builtin | Byte swap |

### Comparison and Predicates (14/14 = 100%)

| Function | Implementation | Notes |
|----------|----------------|-------|
| `cmp` | Pure Julia | Three-way comparison |
| `iseven`, `isodd` | Pure Julia (`math.jl`) | Parity check |
| `iszero`, `isone` | Pure Julia (`number.jl`) | Identity checks |
| `ispow2` | Pure Julia (`intfuncs.jl`) | Power of 2 check |
| `isfinite` | VM Intrinsic | Check if finite |
| `isinf` | VM Intrinsic | Check if infinite |
| `isnan` | VM Intrinsic | Check if NaN |
| `isinteger` | Pure Julia | Check if integer |
| `isreal` | Pure Julia (`number.jl`) | Check if real |
| `isapprox` (≈) | Pure Julia | Approximate equality |
| `≉` | Pure Julia | Approximate inequality |
| `nextpow` | Pure Julia (`intfuncs.jl`) | Next power of base |
| `prevpow` | Pure Julia (`intfuncs.jl`) | Previous power of base |

### Type Conversion and Construction (13/13 = 100%)

| Function | Implementation | Notes |
|----------|----------------|-------|
| `complex` | Rust Builtin | Create complex number |
| `conj` | Rust Builtin / Pure Julia | Complex conjugate |
| `real` | Pure Julia (`number.jl`, `complex.jl`) | Real part |
| `imag` | Pure Julia (`number.jl`, `complex.jl`) | Imaginary part |
| `reim` | Pure Julia (`complex.jl`) | (real, imag) tuple |
| `angle` | Rust Builtin | Phase angle |
| `cis` | Pure Julia (`complex.jl`) | `cos(x) + im*sin(x)` |
| `cispi` | Pure Julia (`complex.jl`) | `cis(π*x)` |
| `zero` | Pure Julia (`number.jl`) | Zero of type |
| `one` | Pure Julia (`number.jl`) | One of type |
| `oneunit` | Pure Julia (`number.jl`) | Multiplicative identity |
| `typemax` | Pure Julia / Builtin | Maximum value of type |
| `typemin` | Pure Julia / Builtin | Minimum value of type |

### Clamp and Range (4/4 = 100%)

| Function | Implementation | Notes |
|----------|----------------|-------|
| `clamp` | Pure Julia (`math.jl`) | Clamp to range |
| `minmax` | Pure Julia (`math.jl`) | Return (min, max) tuple |
| `min` | VM Intrinsic | Minimum of values |
| `max` | VM Intrinsic | Maximum of values |

### Fused Operations (2/2 = 100%)

| Function | Implementation | Notes |
|----------|----------------|-------|
| `fma` | Rust Builtin | Fused multiply-add (single rounding) |
| `muladd` | Rust Builtin | May or may not be fused |

### Other Functions

| Function | Status | Implementation | Notes |
|----------|--------|----------------|-------|
| `evalpoly` | ✅ | Pure Julia (`math.jl`) | Polynomial evaluation |
| `identity` | ✅ | Pure Julia (`number.jl`) | Identity function |
| `inv` | ✅ | Pure Julia | Multiplicative inverse |
| `tryparse` | ✅ | Rust Builtin | Parse string to number |
| `numerator` | ✅ | Pure Julia (`rational.jl`) | Numerator of rational |
| `denominator` | ✅ | Pure Julia (`rational.jl`) | Denominator of rational |
| `deg2rad` | ✅ | Pure Julia (`math.jl`) | Degrees to radians |
| `rad2deg` | ✅ | Pure Julia (`math.jl`) | Radians to degrees |
| `float` | ✅ | Pure Julia (`number.jl`) | Convert to Float64 |
| `signed` | ✅ | Rust Builtin | Convert to signed integer |
| `unsigned` | ✅ | Rust Builtin | Convert to unsigned integer |
| `widemul` | ✅ | Rust Builtin | Wide multiplication |
| `tanpi` | ✅ | Pure Julia (`math.jl`) | `tan(π*x)` with special cases |
| `@evalpoly` | ✅ | Macro lowering (`lowering/stmt/macros.rs`) | Polynomial evaluation macro with variadic arguments |
| `big` | ✅ | Pure Julia (`bigint.jl`) | Convert to BigInt/BigFloat |
| `nextprod` | ✅ | Pure Julia (`intfuncs.jl`) | Next product of factors |
| `rationalize` | ✅ | Pure Julia (`rational.jl`) | Rational approximation |
| `reinterpret` | ✅ | Rust Builtin | Bit-level type reinterpretation for same-size primitives |
| `unsafe_trunc` | ✅ | Pure Julia (`floatfuncs.jl`) | Unsafe truncation (Int64/Int32/Int16/Int8, where clause now supported) |

---

## Unimplemented Functions (Priority Order)

### High Priority (Easy to implement in Pure Julia)

✅ **Completed**: `asech`, `acsch`, `acoth`, `tanpi` have been implemented in Pure Julia.

### Medium Priority

✅ **Completed**: `nextprod` and `rationalize` have been implemented in Pure Julia.
✅ **Completed**: `@evalpoly` macro has been implemented with variadic argument support.

### Low Priority (Require unsafe operations or BigInt)

✅ **Completed**: `big` has been implemented in Pure Julia with support for all numeric types.
✅ **Completed**: `reinterpret` has been implemented as a Rust Builtin for same-size primitive types.

No remaining unimplemented functions in this category.

---

## Implementation Architecture

### Three-Layer Design

```
Layer 3: Pure Julia (math.jl, intfuncs.jl, etc.)
         └─ Most math functions implemented here
         └─ Easy to extend and maintain
         └─ Examples: sinpi, sind, evalpoly, gcd

Layer 2: Rust Builtins (builtins.rs)
         └─ Transcendental functions (sin, cos, exp, log)
         └─ Bit operations (count_ones, bitreverse)
         └─ IEEE 754 operations (nextfloat, frexp)

Layer 1: VM Intrinsics (intrinsics.rs)
         └─ Basic arithmetic (+, -, *, /, ^)
         └─ Comparison (<, <=, ==, >=, >)
         └─ Type predicates (isnan, isinf, isfinite)
```

### When to Use Each Layer

| Use Case | Layer |
|----------|-------|
| Can be expressed as Julia code using existing primitives | Pure Julia |
| Requires native CPU instruction or library call | Rust Builtin |
| Fundamental operation with no Julia equivalent | VM Intrinsic |

---

## References

- Julia `base/exports.jl` lines 255-416: Scalar math function exports
- Julia `base/math.jl`: Most Pure Julia math implementations
- Julia `base/intfuncs.jl`: Integer math functions (gcd, factorial, etc.)
- Julia `base/floatfuncs.jl`: Float-specific functions
- SubsetJuliaVM `src/builtins.rs`: Rust builtin definitions
- SubsetJuliaVM `src/julia/base/math.jl`: Pure Julia math implementations
