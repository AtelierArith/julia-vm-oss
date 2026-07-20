using Test

# Issue #10749: the runtime specializer gained value-level Complex arithmetic on
# its split-slot (SROA) representation, so a Complex operand that reaches
# `emit_binary_op` as a plain value (e.g. a LICM-hoisted loop-invariant temp
# like `__sjulia_licm_0 = ci * im`) no longer aborts the specialization.
#
# The mixed real/complex forms MUST mirror upstream's NARROW methods from
# base/complex.jl (`*(x::Real, z::Complex) = Complex(x*real(z), x*imag(z))`),
# not the full complex product with a zero imaginary part. The two differ on
# non-finite operands — `2.0 * (Inf + 1.0im)` is `Inf + 2.0im` narrow but
# `Inf + NaN*im` under the full formula (the `0 * Inf` cross-term). These tests
# pin that difference so a "simplification" to the full formula is caught.

# Untyped (specialization-eligible) kernels: every operand mix, driven through
# a loop so the runtime specializer actually installs a specialized body.
function mix_add(a, z, n)
    acc = 0.0 + 0.0im
    for _ in 1:n
        acc = acc + (a + z)
    end
    acc
end

function mix_sub_rc(a, z, n)
    acc = 0.0 + 0.0im
    for _ in 1:n
        acc = acc + (a - z)
    end
    acc
end

function mix_sub_cr(a, z, n)
    acc = 0.0 + 0.0im
    for _ in 1:n
        acc = acc + (z - a)
    end
    acc
end

function mix_mul_rc(a, z, n)
    acc = 0.0 + 0.0im
    for _ in 1:n
        acc = acc + a * z
    end
    acc
end

function mix_mul_cr(a, z, n)
    acc = 0.0 + 0.0im
    for _ in 1:n
        acc = acc + z * a
    end
    acc
end

function cx_mul(z, w, n)
    acc = 0.0 + 0.0im
    for _ in 1:n
        acc = acc + z * w
    end
    acc
end

# A loop-invariant Complex built from real locals: this is the LICM shape that
# `mandel_count` hits (`cr + ci * im` with `ci * im` hoisted out of the loop).
function licm_shape(width, height)
    total = 0.0
    for y in 1:height
        ci = 2.0 * y
        for x in 1:width
            cr = 3.0 * x
            total += abs2(cr + ci * im)
        end
    end
    total
end

@testset "specializer mixed real/complex arithmetic (Issue #10749)" begin
    z = 3.0 + 4.0im
    w = 1.0 - 2.0im

    @test mix_add(2.0, z, 3) == 3 * (2.0 + z)
    @test mix_sub_rc(2.0, z, 3) == 3 * (2.0 - z)
    @test mix_sub_cr(2.0, z, 3) == 3 * (z - 2.0)
    @test mix_mul_rc(2.0, z, 3) == 3 * (2.0 * z)
    @test mix_mul_cr(2.0, z, 3) == 3 * (z * 2.0)
    @test cx_mul(z, w, 3) == 3 * (z * w)

    # Integer real operand (must widen, not fall over).
    @test mix_add(2, z, 2) == 2 * (2 + z)
    @test mix_mul_rc(3, z, 2) == 2 * (3 * z)

    # Non-finite operands: pins the NARROW mixed-method semantics.
    zinf = Inf + 1.0im
    @test 2.0 * zinf === Complex(Inf, 2.0)
    @test isequal(mix_mul_rc(2.0, zinf, 1), Complex(Inf, 2.0))
    @test isequal(mix_mul_cr(2.0, zinf, 1), Complex(Inf, 2.0))
    @test isequal(mix_add(1.0, zinf, 1), Complex(Inf, 1.0))

    znan = NaN + 2.0im
    @test isequal(mix_mul_rc(0.0, znan, 1), Complex(NaN, 0.0))

    # LICM-hoisted Complex temp (the mandel_count shape).
    @test licm_shape(3, 3) ≈ 546.0
    @test licm_shape(3, 3) ≈ 546.0
end

true
