# Test divrem(), fldmod(), mod1(), fld1(), fldmod1() functions (Issue #1861)

using Test

@testset "divrem basic" begin
    @test divrem(7, 3) == (2.0, 1)
    @test divrem(10, 5) == (2.0, 0)
    @test divrem(-7, 3) == (-3.0, 2)
end

@testset "fldmod basic" begin
    @test fldmod(7, 3) == (2, 1)
    @test fldmod(10, 5) == (2, 0)
end

@testset "mod1 basic" begin
    @test mod1(1, 3) == 1
    @test mod1(3, 3) == 3
    @test mod1(4, 3) == 1
    @test mod1(6, 3) == 3
end

@testset "fld1 basic" begin
    @test fld1(1, 3) == 0
    @test fld1(3, 3) == 0
    @test fld1(4, 3) == 1
end

@testset "fldmod1 basic" begin
    @test fldmod1(1, 3) == (0, 1)
    @test fldmod1(4, 3) == (1, 1)
    @test fldmod1(6, 3) == (1, 3)
end

true
