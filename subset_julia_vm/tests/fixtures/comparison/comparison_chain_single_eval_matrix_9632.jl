# Chained-comparison single-evaluation prevention matrix (Issue #9632)

using Test

scalar_count_9632 = 0
scalar_bump_9632(x) = (global scalar_count_9632 += 1; x)

type_count_9632 = 0
type_bump_9632(T) = (global type_count_9632 += 1; T)

dot_count_9632 = 0
dot_bump_9632(xs) = (global dot_count_9632 += 1; xs)

short_count_9632 = 0
short_bump_9632(x) = (global short_count_9632 += 1; x)

@testset "scalar comparison-chain operator matrix (Issue #9632)" begin
    global scalar_count_9632 = 0
    @test (0 < scalar_bump_9632(1) < 2)
    @test scalar_count_9632 == 1

    global scalar_count_9632 = 0
    @test (0 <= scalar_bump_9632(0) <= 0)
    @test scalar_count_9632 == 1

    global scalar_count_9632 = 0
    @test (1 == scalar_bump_9632(1) == 1)
    @test scalar_count_9632 == 1

    global scalar_count_9632 = 0
    @test (1 != scalar_bump_9632(2) != 1)
    @test scalar_count_9632 == 1
end

@testset "type comparison-chain operator matrix (Issue #9632)" begin
    global type_count_9632 = 0
    @test (Int <: type_bump_9632(Real) <: Number)
    @test type_count_9632 == 1

    global type_count_9632 = 0
    @test (Number >: type_bump_9632(Integer) >: Int)
    @test type_count_9632 == 1
end

@testset "dotted comparison-chain operator matrix (Issue #9632)" begin
    global dot_count_9632 = 0
    @test (0 .< dot_bump_9632([1, 2]) .< 3) == Bool[1, 1]
    @test dot_count_9632 == 1

    global dot_count_9632 = 0
    @test (0 .<= dot_bump_9632([0, 1]) .<= 1) == Bool[1, 1]
    @test dot_count_9632 == 1

    global dot_count_9632 = 0
    @test (1 .== dot_bump_9632([1, 1]) .== 1) == Bool[1, 1]
    @test dot_count_9632 == 1

    global dot_count_9632 = 0
    @test (1 .!= dot_bump_9632([2, 3]) .!= 1) == Bool[1, 1]
    @test dot_count_9632 == 1
end

@testset "scalar comparison-chain short-circuit count (Issue #9632)" begin
    global short_count_9632 = 0
    r = 10 < short_bump_9632(5) < short_bump_9632(6) < 20
    @test !r
    @test short_count_9632 == 1
end

true
