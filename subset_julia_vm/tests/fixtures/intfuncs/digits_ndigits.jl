# Test digits() and ndigits() with keyword base argument (Issue #1865, #2020)

using Test

@testset "digits base 10 (default)" begin
    d = digits(123)
    @test d[1] == 3
    @test d[2] == 2
    @test d[3] == 1
    @test length(d) == 3

    d0 = digits(0)
    @test d0[1] == 0
    @test length(d0) == 1
end

@testset "digits with keyword base" begin
    # Binary
    d = digits(8, base=2)
    @test length(d) == 4
    @test d[1] == 0
    @test d[4] == 1

    # Hexadecimal (Issue #2020)
    d16 = digits(255, base=16)
    @test length(d16) == 2
    @test d16[1] == 15
    @test d16[2] == 15

    # Binary of 10
    d2 = digits(10, base=2)
    @test length(d2) == 4
    @test d2[1] == 0
    @test d2[2] == 1
    @test d2[3] == 0
    @test d2[4] == 1
end

@testset "ndigits base 10 (default)" begin
    @test ndigits(0) == 1
    @test ndigits(1) == 1
    @test ndigits(9) == 1
    @test ndigits(10) == 2
    @test ndigits(99) == 2
    @test ndigits(100) == 3
    @test ndigits(999) == 3
    @test ndigits(123) == 3
end

@testset "ndigits with keyword base" begin
    @test ndigits(8, base=2) == 4
    @test ndigits(255, base=16) == 2
    @test ndigits(7, base=2) == 3
end

true
