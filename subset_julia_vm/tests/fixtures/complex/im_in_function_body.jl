# Test that im (global const) is correctly typed when used inside function bodies.
# This tests global const type propagation (Issue #3088): before the fix, `im`
# inside a function body would infer as Any instead of Complex{Bool}, causing
# dynamic dispatch failures.

using Test

@testset "im as global const in function bodies" begin
    # im used inside a function body (tests global_types propagation)
    function add_imaginary(x::Float64)
        return x + im
    end

    z = add_imaginary(3.0)
    @test real(z) == 3.0
    @test imag(z) == 1.0

    # im used in multiplication inside a function body
    function rotate_90(z)
        return z * im
    end

    z2 = rotate_90(1.0 + 0.0im)
    @test real(z2) ≈ 0.0 atol=1e-10
    @test imag(z2) ≈ 1.0

    # im used in an expression with a variable
    function make_complex_from_parts(r, i)
        return r + i * im
    end

    z3 = make_complex_from_parts(2.0, 3.0)
    @test real(z3) == 2.0
    @test imag(z3) == 3.0
end

true
