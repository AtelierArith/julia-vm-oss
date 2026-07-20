# Explicit StepRange preserves the user-provided step type parameter.

using Test

@testset "StepRange step type parameter (#9519)" begin
    r_big = big(1):2:big(9)
    @test typeof(r_big) === StepRange{BigInt, Int64}
    @test eltype(r_big) === BigInt
    @test typeof(step(r_big)) === Int64
    @test r_big[2] == big(3)
    @test typeof(r_big[2]) === BigInt

    r_narrow = Int8(1):Int8(1):Int16(5)
    @test typeof(r_narrow) === StepRange{Int16, Int8}
    @test eltype(r_narrow) === Int16
    @test typeof(step(r_narrow)) === Int8
    @test r_narrow[2] === Int16(2)

    r_uint = UInt8(1):UInt8(1):UInt16(5)
    @test typeof(r_uint) === StepRange{UInt16, UInt8}
    @test eltype(r_uint) === UInt16
    @test typeof(step(r_uint)) === UInt8
    @test r_uint[2] === UInt16(2)

    r_big_step = big(1):big(2):big(9)
    @test typeof(r_big_step) === StepRange{BigInt, BigInt}
    @test eltype(r_big_step) === BigInt
    @test typeof(step(r_big_step)) === BigInt

    r_empty = 1:2:0
    @test length(r_empty) == 0
    @test typeof(step(r_empty)) === Int64
    @test step(r_empty) === 2

    r_single = Int8(1):Int8(2):Int16(1)
    @test length(r_single) == 1
    @test typeof(r_single) === StepRange{Int16, Int8}
    @test typeof(step(r_single)) === Int8
    @test step(r_single) === Int8(2)

    r_char = 'a':Int8(1):'c'
    @test typeof(r_char) === StepRange{Char, Int8}
    @test eltype(r_char) === Char
    @test typeof(step(r_char)) === Int8
    @test r_char[2] === 'b'
end

true
