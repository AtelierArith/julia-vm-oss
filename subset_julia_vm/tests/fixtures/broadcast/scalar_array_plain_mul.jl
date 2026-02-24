# Plain scalar * array operations (non-broadcast multiplication)
# Tests that plain `*` works for scalar-array multiplication, same as `.*`.
# Related: Issue #1795

using Test

@testset "Float64 scalar * Float64 array" begin
    a = [1.0, 2.0, 3.0]
    result = 2.0 * a
    @test result == [2.0, 4.0, 6.0]

    # Commutative: array * scalar
    result2 = a * 2.0
    @test result2 == [2.0, 4.0, 6.0]
end

@testset "Int64 scalar * Float64 array" begin
    a = [1.0, 2.0, 3.0]
    result = 3 * a
    @test result == [3.0, 6.0, 9.0]

    result2 = a * 3
    @test result2 == [3.0, 6.0, 9.0]
end

@testset "Float64 scalar * Int64 array" begin
    a = [1, 2, 3]
    result = 2.5 * a
    @test result == [2.5, 5.0, 7.5]

    result2 = a * 2.5
    @test result2 == [2.5, 5.0, 7.5]
end

@testset "Edge cases" begin
    a = [1.0, 2.0, 3.0]

    # Multiply by zero
    @test 0.0 * a == [0.0, 0.0, 0.0]
    @test a * 0.0 == [0.0, 0.0, 0.0]

    # Multiply by one (identity)
    @test 1.0 * a == [1.0, 2.0, 3.0]
    @test a * 1.0 == [1.0, 2.0, 3.0]

    # Negative scalar
    @test -1.0 * a == [-1.0, -2.0, -3.0]
    @test -2.0 * a == [-2.0, -4.0, -6.0]
end

true
