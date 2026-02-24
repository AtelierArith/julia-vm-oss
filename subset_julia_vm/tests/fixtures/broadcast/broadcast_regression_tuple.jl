# Comprehensive regression test for tuple broadcasting
# Validates all tuple broadcast patterns for the Pure Julia migration.
# Related: Issue #2550 (broadcast regression test suite)

using Test

@testset "Tuple-Tuple broadcast operations" begin
    # .+ on tuples
    t1 = (1.0, 2.0, 3.0) .+ (4.0, 5.0, 6.0)
    @test t1 == (5.0, 7.0, 9.0)

    # .- on tuples
    t2 = (10.0, 20.0, 30.0) .- (1.0, 2.0, 3.0)
    @test t2 == (9.0, 18.0, 27.0)

    # .* on tuples
    t3 = (2.0, 3.0, 4.0) .* (5.0, 6.0, 7.0)
    @test t3 == (10.0, 18.0, 28.0)

    # ./ on tuples
    t4 = (10.0, 20.0, 30.0) ./ (2.0, 5.0, 10.0)
    @test t4 == (5.0, 4.0, 3.0)
end

@testset "Tuple-Scalar broadcast" begin
    # Tuple .* scalar
    t1 = (1.0, 2.0, 3.0) .* 2.0
    @test t1 == (2.0, 4.0, 6.0)

    # Scalar .- tuple
    t2 = 10.0 .- (1.0, 2.0, 3.0)
    @test t2 == (9.0, 8.0, 7.0)

    # Scalar .+ tuple
    t3 = 5.0 .+ (1.0, 2.0, 3.0)
    @test t3 == (6.0, 7.0, 8.0)
end

@testset "Tuple type preservation" begin
    # Int64 tuples
    t1 = (1, 2, 3) .+ (4, 5, 6)
    @test t1[1] == 5
    @test typeof(t1[1]) == Int64

    # Int64 .* Int64 -> Int64
    t2 = (2, 3) .* (4, 5)
    @test t2[1] == 8
    @test typeof(t2[1]) == Int64

    # Int64 ./ Int64 -> Float64 (Julia semantics)
    t3 = (4, 6) ./ (2, 3)
    @test t3[1] == 2.0
    @test typeof(t3[1]) == Float64

    # Float64 + Float64 -> Float64
    t4 = (1.0, 2.0) .+ (3.0, 4.0)
    @test typeof(t4[1]) == Float64

    # Int64 + Float64 -> Float64 (promotion)
    t5 = (1, 2) .+ (1.0, 2.0)
    @test t5[1] == 2.0
    @test typeof(t5[1]) == Float64
end

true
