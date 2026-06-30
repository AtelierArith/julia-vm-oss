using Test

@testset "narrow integer identity and membership (#4637)" begin
    @test Int8(1) === Int8(1)
    @test Int16(4) === Int16(4)
    @test Int32(5) === Int32(5)
    @test Int128(6) === Int128(6)
    @test UInt8(2) === UInt8(2)
    @test UInt16(3) === UInt16(3)
    @test UInt32(4) === UInt32(4)
    @test UInt64(5) === UInt64(5)
    @test UInt128(6) === UInt128(6)

    @test !(Int8(1) === Int16(1))
    @test isequal(Int8(1), Int16(1))
    @test isequal(Int8(1), UInt8(1))
    @test hash(Int8(1)) == hash(Int16(1))
    @test hash(Int8(1)) == hash(UInt8(1))

    @test Int8(1) in (Int8(1), Int8(3))
    @test Int16(4) in (Int16(2), Int16(4))
    @test UInt8(2) in (UInt8(1), UInt8(2))
    @test Int8(1) in (Int16(1), Int16(3))
    @test Int8(1) in (UInt8(1), UInt8(3))
    @test !(Int8(2) in (Int8(1), Int8(3)))
end

true
