# Test Generator iterate protocol in Pure Julia
# This tests the Pure Julia Generator implementation using field function calls

using Test

# Define test functions
double(x) = x * 2
square(x) = x * x
add_ten(x) = x + 10

@testset "Generator iterate protocol" begin
    # Basic Generator with array
    g1 = Generator(double, [1, 2, 3])
    result1 = collect(g1)
    @test result1[1] == 2
    @test result1[2] == 4
    @test result1[3] == 6
    @test length(g1) == 3

    # Generator with range
    g2 = Generator(square, 1:4)
    result2 = collect(g2)
    @test result2[1] == 1
    @test result2[2] == 4
    @test result2[3] == 9
    @test result2[4] == 16

    # Generator with another function
    g3 = Generator(add_ten, [5, 15, 25])
    result3 = collect(g3)
    @test result3[1] == 15
    @test result3[2] == 25
    @test result3[3] == 35

    # Empty iterator
    g_empty = Generator(double, Int64[])
    result_empty = collect(g_empty)
    @test length(result_empty) == 0

    # Iterate manually
    g4 = Generator(double, [10, 20])
    r1 = iterate(g4)
    @test r1 !== nothing
    @test r1[1] == 20  # double(10)

    r2 = iterate(g4, r1[2])
    @test r2 !== nothing
    @test r2[1] == 40  # double(20)

    r3 = iterate(g4, r2[2])
    @test r3 === nothing
end

true
