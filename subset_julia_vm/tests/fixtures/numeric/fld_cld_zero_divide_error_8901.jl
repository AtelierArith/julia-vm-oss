using Test

@testset "Int64 fld/cld zero denominator throws DivideError (Issue #8901)" begin
    @test_throws DivideError fld(Int64(-1), Int64(0))
    @test_throws DivideError fld(Int64(1), Int64(0))
    @test_throws DivideError cld(Int64(-1), Int64(0))
    @test_throws DivideError cld(Int64(1), Int64(0))
end

@testset "Int64 fld/cld keep integer rounding" begin
    @test fld(Int64(7), Int64(2)) == Int64(3)
    @test fld(Int64(-7), Int64(2)) == Int64(-4)
    @test fld(Int64(7), Int64(-2)) == Int64(-4)
    @test cld(Int64(7), Int64(2)) == Int64(4)
    @test cld(Int64(-7), Int64(2)) == Int64(-3)
    @test cld(Int64(7), Int64(-2)) == Int64(-3)
end

true
