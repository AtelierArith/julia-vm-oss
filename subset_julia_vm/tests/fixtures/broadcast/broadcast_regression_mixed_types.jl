# Regression test for mixed numeric type broadcasting
# Validates type promotion rules in broadcast operations.
# Related: Issue #2550 (broadcast regression test suite)

using Test

@testset "Int64-Float64 promotion in broadcast" begin
    # Int64 array .+ Float64 scalar -> Float64 array
    a = [1, 2, 3]
    result = a .+ 0.5
    @test result == [1.5, 2.5, 3.5]
    @test typeof(result[1]) == Float64

    # Float64 array .* Int64 scalar -> Float64 array
    b = [1.0, 2.0, 3.0]
    result2 = b .* 2
    @test result2 == [2.0, 4.0, 6.0]
    @test typeof(result2[1]) == Float64
end

@testset "Int64-Float64 array broadcast" begin
    a = [1, 2, 3]
    b = [1.5, 2.5, 3.5]

    # Int64 array .+ Float64 array -> Float64 array
    result = a .+ b
    @test result == [2.5, 4.5, 6.5]
    @test typeof(result[1]) == Float64
end

@testset "broadcast() function with mixed types" begin
    f(x, y) = x + y

    # broadcast with Int64 and Float64 arrays
    result = broadcast(f, [1, 2, 3], [0.5, 1.5, 2.5])
    @test result[1] == 1.5
    @test result[2] == 3.5
    @test result[3] == 5.5
end

true
