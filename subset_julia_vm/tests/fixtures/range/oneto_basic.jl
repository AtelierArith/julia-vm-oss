# Test OneTo basic operations (Issue #325)
# OneTo(n) represents range 1:n with guaranteed start at 1

using Test

@testset "OneTo basic operations" begin
    r = OneTo(5)

    # Type check
    @test isa(r, OneTo)

    # Length
    @test length(r) == 5

    # First is always 1
    @test first(r) == 1

    # Last
    @test last(r) == 5

    # Step is always 1
    @test step(r) == 1
end

@testset "OneTo iteration" begin
    r = OneTo(4)
    values = Int64[]
    for x in r
        push!(values, x)
    end

    @test length(values) == 4
    @test values[1] == 1
    @test values[4] == 4
end

@testset "OneTo getindex" begin
    r = OneTo(5)

    @test r[1] == 1
    @test r[3] == 3
    @test r[5] == 5
end

@testset "OneTo empty range" begin
    r = OneTo(0)

    @test length(r) == 0
    @test isempty(r)
end

@testset "oneto convenience constructor" begin
    r = oneto(5)

    @test isa(r, OneTo)
    @test length(r) == 5
end

true
