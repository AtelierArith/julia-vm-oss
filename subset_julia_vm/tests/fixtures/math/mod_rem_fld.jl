# Test mod(), rem(), fld() functions (Issue #1877)

using Test

@testset "mod positive" begin
    @test mod(7, 3) == 1
    @test mod(6, 3) == 0
    @test mod(8, 3) == 2
end

@testset "mod negative dividend" begin
    # mod result has same sign as divisor
    @test mod(-7, 3) == 2
    @test mod(-1, 3) == 2
end

@testset "mod negative divisor" begin
    @test mod(7, -3) == -2
    @test mod(-7, -3) == -1
end

@testset "mod float" begin
    @test abs(mod(5.5, 2.0) - 1.5) < 1e-14
    @test abs(mod(-5.5, 2.0) - 0.5) < 1e-14
end

@testset "rem basic" begin
    @test rem(7, 3) == 1
    @test rem(6, 3) == 0
    @test rem(-7, 3) == -1
    @test rem(7, -3) == 1
end

@testset "fld integer" begin
    @test fld(7, 3) == 2
    @test fld(6, 3) == 2
    @test fld(-7, 3) == -3
    @test fld(7, -3) == -3
end

@testset "fld float" begin
    @test fld(5.5, 2.0) == 2.0
    @test fld(-5.5, 2.0) == -3.0
end

true
