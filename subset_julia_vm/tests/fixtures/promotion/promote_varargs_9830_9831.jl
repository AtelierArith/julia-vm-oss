using Test

@testset "promote_type varargs (Issue #9830)" begin
    @test promote_type(Int64) === Int64
    @test promote_type((Int64,)...) === Int64

    wide_types = (Int64, Float64, Float32, Int8, UInt16)
    @test promote_type(wide_types...) === Float64
    @test promote_type(Int8, Int16, Int32, Int64, Int128, UInt8) === Int128
end

@testset "promote varargs (Issue #9831)" begin
    mixed = promote(1, 1.0, 2, 3)
    @test mixed == (1.0, 1.0, 2.0, 3.0)
    @test typeof(mixed) === Tuple{Float64, Float64, Float64, Float64}

    wider = promote(Int8(1), Int16(2), Int32(3), Int64(4), UInt8(5))
    @test wider == (1, 2, 3, 4, 5)
    @test typeof(wider) === Tuple{Int64, Int64, Int64, Int64, Int64}

    same = promote(1, 2, 3, 4)
    @test same === (1, 2, 3, 4)
end

true
