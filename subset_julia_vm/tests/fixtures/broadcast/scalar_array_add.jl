# Scalar + Array and Scalar - Array operations (non-broadcast)
# Tests scalar-array addition and subtraction for various numeric types.
# Related: Issue #1601

using Test

@testset "Float64 scalar + Float64 array" begin
    a = [1.0, 2.0, 3.0]
    result = 10.0 .+ a
    @test result == [11.0, 12.0, 13.0]

    # Array + scalar
    result2 = a .+ 10.0
    @test result2 == [11.0, 12.0, 13.0]
end

@testset "Float64 scalar - Float64 array" begin
    a = [1.0, 2.0, 3.0]
    result = 10.0 .- a
    @test result == [9.0, 8.0, 7.0]

    # Array - scalar
    result2 = a .- 1.0
    @test result2 == [0.0, 1.0, 2.0]
end

@testset "Int64 scalar + Float64 array" begin
    a = [1.0, 2.0, 3.0]
    result = 5 .+ a
    @test result == [6.0, 7.0, 8.0]
end

@testset "Float64 scalar + Int64 array" begin
    a = [1, 2, 3]
    result = 0.5 .+ a
    @test result == [1.5, 2.5, 3.5]
end

@testset "Scalar subtraction edge cases" begin
    a = [1.0, 2.0, 3.0]

    # Subtracting zero
    @test 0.0 .+ a == a
    @test a .- 0.0 == a
end

true
