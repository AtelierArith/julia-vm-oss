# Issue #10645: the concrete `Base.:+(z::Complex{Float64}, w::Complex{Float64})`
# and `Base.:*(z::Complex{Float64}, w::Complex{Float64})` overloads
# (`subset_julia_vm/src/julia/base/complex.jl`) use direct `z.re`/`z.im` field
# access and the concrete `Complex{Float64}(r, i)` constructor instead of the
# generic `real`/`imag`-dispatching bodies, so they predecode into a
# frame-less `TypedScalarFunctionBlock` (static `GetField`/`AddF64`/`MulF64`/
# `SubF64` + `NewStruct`, mirroring `abs2(z::Complex{Float64})`). This fixture
# locks IEEE correctness (NaN / +-Inf / signed zero) against upstream
# julia-1.12 byte-for-byte, verified with
# `julia --startup-file=no complex_f64_binary_predecode_10645.jl` producing
# the same pass/fail pattern.

tests_passed = true

# Helper: exact float identity including signed zero and NaN bit pattern
# (upstream Julia's `===` on Float64 compares bits, so `-0.0 === 0.0` is
# false and `NaN === NaN` is true for the canonical NaN payload produced by
# ordinary arithmetic).
function part_eq(actual::Float64, expected::Float64)
    if isnan(expected)
        return isnan(actual)
    end
    return actual === expected
end

function check(z::Complex{Float64}, expected_re::Float64, expected_im::Float64)
    return part_eq(real(z), expected_re) && part_eq(imag(z), expected_im)
end

# --- Ordinary finite values (`+` and `*`), verified against upstream julia.
r1 = (1.0 + 2.0im) + (3.0 - 4.0im)
global tests_passed = tests_passed && check(r1, 4.0, -2.0)

r2 = (1.0 + 2.0im) * (3.0 - 4.0im)
global tests_passed = tests_passed && check(r2, 11.0, 2.0)

# Operand order: `*` is not literally commutative in evaluation order (the
# concrete overload always computes `z.re*w.re - z.im*w.im`, `z.re*w.im +
# z.im*w.re`), but the *value* must match swapping z/w, exactly as upstream.
r2_swapped = (3.0 - 4.0im) * (1.0 + 2.0im)
global tests_passed = tests_passed && check(r2_swapped, 11.0, 2.0)

# --- Signed zero: `+0.0 + -0.0 == +0.0`; `-0.0 + -0.0 == -0.0` (IEEE 754
# addition sign rules), each component independent.
r3 = (0.0 + 0.0im) + (-0.0 + 0.0im)
global tests_passed = tests_passed && check(r3, 0.0, 0.0)

r4 = (-0.0 - 0.0im) + (-0.0 - 0.0im)
global tests_passed = tests_passed && check(r4, -0.0, -0.0)

# `-0.0 + 1.0im` is `(-0.0) + Complex(0.0, 1.0)` (real+Complex `+`), and IEEE
# `-0.0 + 0.0 == +0.0`, so the operand's real part is `+0.0`, not `-0.0` —
# verified against upstream. `z.re*w.re - z.im*w.im == 0.0*1.0 - 1.0*0.0 ==
# 0.0`; imaginary part `0.0*0.0 + 1.0*1.0 == 1.0`.
r5 = (-0.0 + 1.0im) * (1.0 + 0.0im)
global tests_passed = tests_passed && check(r5, 0.0, 1.0)
global tests_passed = tests_passed && !signbit(real(r5))
global tests_passed = tests_passed && !signbit(real(-0.0 + 1.0im))

# A genuine negative-zero real part (built via the `Complex(re, im)`
# constructor, which does not renormalize the sign the way real+Complex `+`
# does above): `z.re*w.re - z.im*w.im == (-0.0)*1.0 - 1.0*0.0 == -0.0`.
z_negzero = Complex(-0.0, 1.0)
w_one = Complex(1.0, 0.0)
r5b = z_negzero * w_one
global tests_passed = tests_passed && check(r5b, -0.0, 1.0)
global tests_passed = tests_passed && signbit(real(r5b))

# --- +-Inf propagation.
r6 = (Inf + 2.0im) + (-Inf + 1.0im)
global tests_passed = tests_passed && check(r6, NaN, 3.0)
global tests_passed = tests_passed && isnan(real(r6))

r7 = (Inf + 0.0im) * (0.0 + 1.0im)
global tests_passed = tests_passed && check(r7, NaN, Inf)
global tests_passed = tests_passed && isnan(real(r7))
global tests_passed = tests_passed && isinf(imag(r7))

# --- NaN propagation (NaN in either operand poisons both output components
# through the `re*re - im*im`/`re*im + im*re` cross terms for `*`, and
# directly for `+`).
r8 = (NaN + 1.0im) + (2.0 + 3.0im)
global tests_passed = tests_passed && isnan(real(r8))
global tests_passed = tests_passed && imag(r8) === 4.0

r9 = (NaN + 0.0im) * (0.0 + 0.0im)
global tests_passed = tests_passed && isnan(real(r9))
global tests_passed = tests_passed && isnan(imag(r9))

# --- Mixed Complex-Real operands still route through the generic
# `Complex{T}, Real` methods (untouched by this issue) and must stay correct
# alongside the new concrete `Complex{Float64}, Complex{Float64}` overloads.
r10 = (1.0 + 2.0im) + 3.0
global tests_passed = tests_passed && check(r10, 4.0, 2.0)

r11 = 3.0 * (1.0 + 2.0im)
global tests_passed = tests_passed && check(r11, 3.0, 6.0)

# --- A short accumulation loop (repeated `*`/`+` dispatch at the same call
# site, the shape the frame-less fast path targets) checked against a
# directly-computed reference using the same formula, guarding against any
# drift between the fast path and a fresh per-call computation.
function accumulate(zs::Vector{ComplexF64})
    acc = zero(ComplexF64)
    for z in zs
        acc = acc * z + z
    end
    return acc
end

zs = ComplexF64[1.0 + 1.0im, 2.0 - 1.0im, -1.0 + 0.5im, 0.0 + 0.0im, 3.0 + 3.0im]
acc_ref = 0.0 + 0.0im
for z in zs
    global acc_ref = Complex(real(acc_ref) * real(z) - imag(acc_ref) * imag(z),
                              real(acc_ref) * imag(z) + imag(acc_ref) * real(z)) + z
end
r12 = accumulate(zs)
global tests_passed = tests_passed && check(r12, real(acc_ref), imag(acc_ref))

tests_passed
