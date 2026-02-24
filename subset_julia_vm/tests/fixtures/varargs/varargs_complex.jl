# Test varargs parameters with Complex type annotations (Issue #1684)
# Typed varargs like (x::Complex{Float64}, ys::Complex{Float64}...) should work correctly

using Test

# Complex varargs with Float64
function sum_complex(x::Complex{Float64}, ys::Complex{Float64}...)
    total = x
    for y in ys
        total += y
    end
    total
end

function count_complex(xs::Complex{Float64}...)
    length(xs)
end

# Mixed: regular type with Complex varargs
function prepend_real(r::Float64, cs::Complex{Float64}...)
    # Return tuple of real part and count of complex numbers
    (r, length(cs))
end

# Pre-compute test values outside testset
c1 = 1.0 + 2.0im
c2 = 3.0 + 4.0im
c3 = 5.0 + 6.0im

sum_result_1 = sum_complex(c1, c2)
sum_result_2 = sum_complex(c1, c2, c3)
sum_result_3 = sum_complex(c1)

expected_1 = 4.0 + 6.0im
expected_2 = 9.0 + 12.0im

@testset "Complex type varargs parameters" begin
    # Test count_complex
    @test count_complex() == 0
    @test count_complex(1.0 + 2.0im) == 1
    @test count_complex(1.0 + 2.0im, 3.0 + 4.0im) == 2
    @test count_complex(1.0 + 0.0im, 2.0 + 0.0im, 3.0 + 0.0im) == 3

    # Test sum_complex - comparing whole complex values
    @test sum_result_1 == expected_1
    @test sum_result_2 == expected_2
    @test sum_result_3 == c1

    # Test prepend_real
    @test prepend_real(10.0) == (10.0, 0)
    @test prepend_real(10.0, 1.0 + 2.0im) == (10.0, 1)
    @test prepend_real(10.0, 1.0 + 2.0im, 3.0 + 4.0im) == (10.0, 2)
end

true
