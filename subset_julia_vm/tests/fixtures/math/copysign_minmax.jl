# Test copysign() and minmax() functions (Issue #1861)

using Test

@testset "copysign basic" begin
    @test copysign(3.0, 1.0) == 3.0
    @test copysign(3.0, -1.0) == -3.0
    @test copysign(-3.0, 1.0) == 3.0
    @test copysign(-3.0, -1.0) == -3.0
end

@testset "minmax basic" begin
    @test minmax(3, 5) == (3, 5)
    @test minmax(5, 3) == (3, 5)
    @test minmax(2.0, 2.0) == (2.0, 2.0)
    @test minmax(-1, 1) == (-1, 1)
end

true
