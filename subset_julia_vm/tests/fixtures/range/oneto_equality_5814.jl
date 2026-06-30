using Test
@testset "OneTo equality (Issue #5814)" begin
    @test Base.OneTo(3) == Base.OneTo(3)
    @test !(Base.OneTo(3) == Base.OneTo(4))
    @test Base.OneTo(3) != Base.OneTo(4)
    @test Base.OneTo(3) == 1:3
    @test 1:3 == Base.OneTo(3)
    @test !(Base.OneTo(3) == 1:4)
    @test Base.OneTo(0) == Base.OneTo(0)
    # Inside a function (slot-typed operands)
    f() = Base.OneTo(5) == Base.OneTo(5)
    @test f()
    g(a, b) = a == b
    @test g(Base.OneTo(3), Base.OneTo(3))
    @test !g(Base.OneTo(3), Base.OneTo(2))
end
true
