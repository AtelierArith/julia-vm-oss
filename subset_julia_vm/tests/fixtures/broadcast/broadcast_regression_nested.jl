# Regression test for nested broadcast operations
# Tests compound dot expressions like sin.(x) .+ cos.(x)
# Related: Issue #2550 (broadcast regression test suite)

using Test

@testset "Nested unary + binary broadcast" begin
    x = [0.0]

    # sin.(x) .+ cos.(x) should equal sin(0) + cos(0) = 0 + 1 = 1
    result = sin.(x) .+ cos.(x)
    @test result[1] == 1.0

    # sqrt.(x) .* 2 where x has perfect squares
    y = [4.0, 9.0, 16.0]
    result2 = sqrt.(y) .* 2.0
    @test result2 == [4.0, 6.0, 8.0]
end

@testset "Chained binary broadcast" begin
    a = [1.0, 2.0, 3.0]
    b = [4.0, 5.0, 6.0]
    c = [10.0, 20.0, 30.0]

    # (a .+ b) .* c
    result = (a .+ b) .* c
    @test result == [50.0, 140.0, 270.0]

    # a .* b .+ c (left to right evaluation)
    result2 = a .* b .+ c
    @test result2 == [14.0, 30.0, 48.0]
end

@testset "Broadcast with scalar combinations" begin
    a = [1.0, 2.0, 3.0]

    # (a .+ 1.0) .* 2.0
    result = (a .+ 1.0) .* 2.0
    @test result == [4.0, 6.0, 8.0]

    # 2.0 .* a .+ 1.0
    result2 = 2.0 .* a .+ 1.0
    @test result2 == [3.0, 5.0, 7.0]
end

true
