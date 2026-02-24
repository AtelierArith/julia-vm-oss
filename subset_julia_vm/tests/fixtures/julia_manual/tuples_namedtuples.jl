# Julia Manual: Tuples and Named Tuples
# Part of Collections (https://docs.julialang.org/en/v1/manual/functions/#Tuples)
# Tests tuple construction, indexing, destructuring, and named tuples.

using Test

@testset "Tuple basics" begin
    t = (1, 2, 3)
    @test t[1] == 1
    @test t[2] == 2
    @test t[3] == 3
    @test length(t) == 3
end

@testset "Mixed-type tuples" begin
    t = (1, "hello", 3.14)
    @test t[1] == 1
    @test t[2] == "hello"
    @test t[3] == 3.14
end

@testset "Tuple destructuring" begin
    a, b, c = (10, 20, 30)
    @test a == 10
    @test b == 20
    @test c == 30
end

@testset "Single-element tuple" begin
    t = (42,)
    @test t[1] == 42
    @test length(t) == 1
end

@testset "Named tuples" begin
    nt = (name="Alice", age=30)
    @test nt.name == "Alice"
    @test nt.age == 30
end

@testset "Tuple equality" begin
    @test (1, 2, 3) == (1, 2, 3)
    @test (1, 2, 3) != (1, 2, 4)
end

@testset "Tuple iteration" begin
    t = (1, 2, 3, 4, 5)
    s = 0
    for x in t
        s += x
    end
    @test s == 15
end

@testset "Tuple from function" begin
    function minmax(a, b)
        a < b ? (a, b) : (b, a)
    end
    lo, hi = minmax(5, 3)
    @test lo == 3
    @test hi == 5
end

true
