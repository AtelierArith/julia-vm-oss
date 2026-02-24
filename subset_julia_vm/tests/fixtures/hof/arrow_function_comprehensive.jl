# Comprehensive test for arrow function support (Issue #1587)

using Test

@testset "Arrow function support" begin
    # Basic arrow function in map
    result1 = map(x -> x * 2, [1, 2, 3])
    @test result1[1] == 2
    @test result1[2] == 4
    @test result1[3] == 6

    # Arrow function with underscore (unused parameter)
    result2 = map(_ -> 42, [1, 2, 3])
    @test result2[1] == 42
    @test result2[2] == 42
    @test result2[3] == 42

    # Arrow function assigned to variable
    double = x -> x * 2
    @test double(5) == 10
    @test double(3) == 6

    # Arrow function with multiple parameters (tuple)
    add = (a, b) -> a + b
    @test add(3, 4) == 7

    # Arrow function returning complex value
    make_complex = x -> Complex(x, x * 2)
    z = make_complex(3.0)
    @test real(z) == 3.0
    @test imag(z) == 6.0

    # Arrow function in filter
    evens = filter(x -> x % 2 == 0, [1, 2, 3, 4, 5, 6])
    @test length(evens) == 3
    @test evens[1] == 2
    @test evens[2] == 4
    @test evens[3] == 6

    # Arrow function creating complex array
    complex_array = map(i -> Complex(Float64(i), 0.0), 1:3)
    @test length(complex_array) == 3
    @test real(complex_array[1]) == 1.0
    @test real(complex_array[2]) == 2.0
    @test real(complex_array[3]) == 3.0
end

true
