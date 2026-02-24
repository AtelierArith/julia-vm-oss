# Regression test for unary function broadcast
# Validates dot-call syntax for unary functions.
# Related: Issue #2550 (broadcast regression test suite)

using Test

@testset "Unary math function broadcast" begin
    # sqrt.()
    @test sqrt.([1.0, 4.0, 9.0]) == [1.0, 2.0, 3.0]

    # abs.()
    @test abs.([-1.0, 2.0, -3.0]) == [1.0, 2.0, 3.0]

    # sin.() and cos.()
    @test sin.([0.0]) == [0.0]
    @test cos.([0.0]) == [1.0]
end

@testset "Unary function broadcast with Int64 arrays" begin
    # abs.() with Int64
    result = abs.([-1, -2, 3])
    @test result == [1, 2, 3]

    # Float64 functions applied to Int64 arrays produce Float64
    result2 = sqrt.([1, 4, 9])
    @test result2 == [1.0, 2.0, 3.0]
    @test typeof(result2[1]) == Float64
end

@testset "User-defined function broadcast" begin
    double(x) = x * 2
    @test double.([1.0, 2.0, 3.0]) == [2.0, 4.0, 6.0]

    square(x) = x * x
    @test square.([2.0, 3.0, 4.0]) == [4.0, 9.0, 16.0]
end

true
