# Julia Manual: Multi-dimensional Arrays
# https://docs.julialang.org/en/v1/manual/arrays/
# Tests array construction, indexing, operations, and comprehensions.

using Test

@testset "Array construction" begin
    a = [1, 2, 3]
    @test length(a) == 3
    @test a[1] == 1
    @test a[3] == 3

    b = [1.0, 2.0, 3.0]
    @test typeof(b[1]) == Float64

    empty_arr = Int64[]
    @test length(empty_arr) == 0
end

@testset "Array operations" begin
    a = [1, 2, 3, 4, 5]
    @test length(a) == 5
    @test sum(a) == 15

    push!(a, 6)
    @test length(a) == 6
    @test a[6] == 6

    pop!(a)
    @test length(a) == 5
end

@testset "Array indexing" begin
    a = [10, 20, 30, 40, 50]
    @test a[1] == 10
    @test a[end] == 50
    @test a[2:4] == [20, 30, 40]
end

@testset "Array comprehensions" begin
    squares = [x^2 for x in 1:5]
    @test squares == [1, 4, 9, 16, 25]

    evens = [x for x in 1:10 if x % 2 == 0]
    @test evens == [2, 4, 6, 8, 10]
end

@testset "Matrix operations" begin
    A = [1 2; 3 4]
    @test size(A) == (2, 2)
    @test A[1, 1] == 1
    @test A[2, 2] == 4
    @test A[1, 2] == 2
end

@testset "Array functions" begin
    a = [3, 1, 4, 1, 5, 9, 2, 6]
    @test minimum(a) == 1
    @test maximum(a) == 9
    @test sort(a) == [1, 1, 2, 3, 4, 5, 6, 9]
    @test reverse(a) == [6, 2, 9, 5, 1, 4, 1, 3]
end

@testset "Zeros and ones" begin
    z = zeros(3)
    @test length(z) == 3
    @test z[1] == 0.0

    o = ones(3)
    @test o[1] == 1.0
end

@testset "Broadcasting" begin
    a = [1.0, 2.0, 3.0]
    b = a .* 2.0
    @test b == [2.0, 4.0, 6.0]

    c = a .+ 10.0
    @test c == [11.0, 12.0, 13.0]

    d = sin.(a)
    @test length(d) == 3
end

@testset "map and filter" begin
    a = [1, 2, 3, 4, 5]
    doubled = map(x -> 2x, a)
    @test doubled == [2, 4, 6, 8, 10]

    odds = filter(x -> x % 2 != 0, a)
    @test odds == [1, 3, 5]
end

true
