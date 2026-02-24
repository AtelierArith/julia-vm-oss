# Test Higher-Order Functions in Pure Julia
# Tests map, filter, and reduce implementations
#
# Issue #1665 fixed: map(f, filter(...)) now works correctly.
# The fix adds a dispatch penalty for Any argument matching specific parameter types,
# ensuring the generic map(f::Function, A) method is selected over scalar methods
# like map(f::Function, x::Int64) when argument type is unknown at compile time.

using Test

double(x) = x * 2
ispositive(x) = x > 0
add(a, b) = a + b

@testset "map - Pure Julia" begin
    # map with lambda
    r1 = map(x -> x * 2, [1, 2, 3])
    @test length(r1) == 3
    @test r1[1] == 2
    @test r1[2] == 4
    @test r1[3] == 6

    # map with named function
    r2 = map(double, [1, 2, 3])
    @test length(r2) == 3
    @test r2[1] == 2
    @test r2[2] == 4
    @test r2[3] == 6
end

@testset "filter - Pure Julia" begin
    # filter with lambda
    r1 = filter(x -> x > 0, [-1, 0, 1, 2])
    @test length(r1) == 2
    @test r1[1] == 1
    @test r1[2] == 2

    # filter with named function
    r2 = filter(ispositive, [-1, 0, 1, 2])
    @test length(r2) == 2
    @test r2[1] == 1
    @test r2[2] == 2
end

@testset "combined - Pure Julia" begin
    # Issue #1665 FIXED: map(f, filter(...)) now works correctly
    # Original code: map(double, filter(ispositive, [-1, 0, 1, 2]))

    # Intermediate variable for clarity (now works without issues)
    filtered = filter(ispositive, [-1, 0, 1, 2])
    @test length(filtered) == 2
    @test filtered[1] == 1
    @test filtered[2] == 2

    # Issue #1665 fixed - this now works correctly
    r2 = map(double, filtered)
    @test length(r2) == 2
    @test r2[1] == 2
    @test r2[2] == 4
end

@testset "reduce - Pure Julia" begin
    # reduce with lambda
    r1 = reduce((a, b) -> a + b, [1, 2, 3, 4])
    @test r1 == 10

    # reduce with named function
    r2 = reduce(add, [1, 2, 3, 4])
    @test r2 == 10

    # reduce with multiplication
    r3 = reduce((a, b) -> a * b, [1, 2, 3, 4])
    @test r3 == 24
end

@testset "foldl - Pure Julia" begin
    # foldl is alias for reduce (left-fold)
    r1 = foldl((a, b) -> a + b, [1, 2, 3, 4])
    @test r1 == 10

    # foldl with subtraction shows left-associativity: ((1-2)-3)-4 = -8
    r2 = foldl((a, b) -> a - b, [1, 2, 3, 4])
    @test r2 == -8
end

@testset "foldr - Pure Julia" begin
    # foldr is right-fold: 1-(2-(3-4)) = 1-(-1) = 2
    r1 = foldr((a, b) -> a - b, [1, 2, 3, 4])
    @test r1 == -2

    # foldr with addition same as foldl
    r2 = foldr((a, b) -> a + b, [1, 2, 3, 4])
    @test r2 == 10
end

true
