# UnitRange step preserves the promoted element type, including empty ranges.

using Test

@testset "UnitRange step type (#9811)" begin
    r_uint = UInt8(1):UInt16(5)
    @test typeof(r_uint) === UnitRange{UInt16}
    @test eltype(r_uint) === UInt16
    @test typeof(step(r_uint)) === UInt16
    @test step(r_uint) === UInt16(1)

    r_int = Int8(1):Int16(5)
    @test typeof(r_int) === UnitRange{Int16}
    @test eltype(r_int) === Int16
    @test typeof(step(r_int)) === Int16
    @test step(r_int) === Int16(1)

    empty_int = Int16(5):Int8(1)
    @test length(empty_int) == 0
    @test typeof(empty_int) === UnitRange{Int16}
    @test eltype(empty_int) === Int16
    @test typeof(step(empty_int)) === Int16
    @test step(empty_int) === Int16(1)

    empty_uint = UInt16(5):UInt8(1)
    @test length(empty_uint) == 0
    @test typeof(empty_uint) === UnitRange{UInt16}
    @test eltype(empty_uint) === UInt16
    @test typeof(step(empty_uint)) === UInt16
    @test step(empty_uint) === UInt16(1)

    r_big = big(1):big(3)
    @test typeof(r_big) === UnitRange{BigInt}
    @test eltype(r_big) === BigInt
    @test typeof(step(r_big)) === BigInt
    @test step(r_big) == big(1)
end

true
