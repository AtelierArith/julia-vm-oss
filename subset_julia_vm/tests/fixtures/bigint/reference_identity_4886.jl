using Test

@testset "BigInt reference identity (Issue #4886)" begin
    x = big(7)
    y = big(7)
    alias = x

    @test x === x
    @test alias === x
    @test !(x === y)
    @test x == y

    ctor_x = BigInt(7)
    ctor_y = BigInt(7)
    ctor_alias = ctor_x

    @test ctor_alias === ctor_x
    @test !(ctor_x === ctor_y)
    @test ctor_x == ctor_y
end

@testset "BigFloat reference identity (Issue #4886)" begin
    x = big(7.0)
    y = big(7.0)
    alias = x

    @test x === x
    @test alias === x
    @test !(x === y)
    @test x == y

    ctor_x = BigFloat(7)
    ctor_y = BigFloat(7)
    ctor_alias = ctor_x

    @test ctor_alias === ctor_x
    @test !(ctor_x === ctor_y)
    @test ctor_x == ctor_y
end

true
