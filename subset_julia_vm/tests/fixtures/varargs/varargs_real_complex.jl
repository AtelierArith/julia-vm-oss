# Regression test for Issue #1717
# real() should return Float64 for Complex{Float64}, not Complex{Float64}
# This bug occurred when real() was called on the result of a typed varargs
# function with a for loop, because empty varargs led to Bottom type propagation
# that widened the return type to Top during type inference.

using Test

# Typed varargs function with for loop - the bug scenario
function sum_complex_varargs(x::Complex{Float64}, ys::Complex{Float64}...)
    total = x
    for y in ys
        total += y
    end
    return total
end

@testset "Issue #1717: real() on varargs function result" begin
    c = 1.0 + 2.0im

    # Test 1: Direct call to real()
    @test real(c) == 1.0
    @test typeof(real(c)) == Float64

    # Test 2: real() on result of varargs function with no extra args
    # This is the scenario that triggered the bug (empty varargs tuple)
    result = sum_complex_varargs(c)
    @test result == c
    @test real(result) == 1.0
    @test typeof(real(result)) == Float64

    # Test 3: real() on result of varargs function with extra args
    c2 = 3.0 + 4.0im
    result2 = sum_complex_varargs(c, c2)
    @test real(result2) == 4.0
    @test typeof(real(result2)) == Float64

    # Test 4: imag() should also work correctly
    @test imag(result) == 2.0
    @test typeof(imag(result)) == Float64
end

true
