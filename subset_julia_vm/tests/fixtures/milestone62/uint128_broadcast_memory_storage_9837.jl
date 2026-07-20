using Test

@testset "UInt128 broadcast and typed storage" begin
    values = Vector{UInt128}(undef, 2)
    values[1] = UInt128(1) << 64
    values[2] = UInt128(5)
    @test typeof(values[1]) == UInt128
    @test values[1] == UInt128(1) << 64
    @test typeof(values[2]) == UInt128

    powered = UInt128[2, 3] .^ UInt8[64, 2]
    @test typeof(powered) == Vector{UInt128}
    @test typeof(powered[1]) == UInt128
    @test powered[1] == UInt128(1) << 64
    @test powered[2] == UInt128(9)
end

true
