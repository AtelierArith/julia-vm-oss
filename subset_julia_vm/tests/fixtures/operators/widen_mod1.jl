# Test widen, mod1, fld1, fldmod1 functions (Issue #1883)

using Test

@testset "widen Int32" begin
    x = Int32(42)
    w = widen(x)
    @test w == 42
    @test isa(w, Int64)
end

@testset "widen Float32" begin
    x = Float32(3.14)
    w = widen(x)
    @test abs(w - 3.14) < 0.01
    @test isa(w, Float64)
end

@testset "widen type Int32" begin
    @test widen(Int32) == Int64
end

@testset "widen type Float32" begin
    @test widen(Float32) == Float64
end

@testset "mod1 integer" begin
    @test mod1(4, 3) == 1
    @test mod1(3, 3) == 3
    @test mod1(6, 3) == 3
    @test mod1(1, 3) == 1
    @test mod1(7, 4) == 3
end

@testset "mod1 float" begin
    @test mod1(4.0, 3.0) == 1.0
    @test mod1(3.0, 3.0) == 3.0
    @test mod1(6.0, 3.0) == 3.0
end

@testset "fld1 integer" begin
    @test fld1(1, 3) == 1
    @test fld1(3, 3) == 1
    @test fld1(4, 3) == 2
    @test fld1(6, 3) == 2
    @test fld1(7, 3) == 3
end

@testset "fldmod1 integer" begin
    d, m = fldmod1(7, 3)
    @test d == 3
    @test m == 1
end

@testset "fldmod1 relation" begin
    # x == (fld1(x,y) - 1)*y + mod1(x,y)
    x = 10
    y = 3
    d, m = fldmod1(x, y)
    @test x == (d - 1) * y + m
end

true
