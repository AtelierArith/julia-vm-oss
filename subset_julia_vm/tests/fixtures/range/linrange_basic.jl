# Test LinRange basic operations (Issue #325)
# LinRange provides lazy linearly-spaced values between start and stop

using Test

@testset "LinRange basic operations" begin
    r = LinRange(1.0, 10.0, 5)

    # Type check
    @test isa(r, LinRange)

    # Length
    @test length(r) == 5

    # First and last
    @test first(r) == 1.0
    @test last(r) == 10.0

    # Step (computed from lendiv)
    @test step(r) == 2.25
end

@testset "LinRange iteration" begin
    r = LinRange(1.0, 10.0, 5)
    values = Float64[]
    for x in r
        push!(values, x)
    end

    @test length(values) == 5
    @test values[1] == 1.0
    @test values[5] == 10.0
end

@testset "LinRange getindex" begin
    r = LinRange(0.0, 1.0, 5)

    @test r[1] == 0.0
    @test r[5] == 1.0
    @test r[3] == 0.5
end

@testset "LinRange from range function" begin
    # range(start, stop, length) should return LinRange
    r = range(1.0, 10.0, 5)

    @test isa(r, LinRange)
    @test first(r) == 1.0
    @test last(r) == 10.0
    @test length(r) == 5
end

true
