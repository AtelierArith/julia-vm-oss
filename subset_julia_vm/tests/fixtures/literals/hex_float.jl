# Hex float literal test (with p/P exponent for power of 2)

using Test

@testset "Hex float literals with p exponent: 0x1p0, 0x1.8p3" begin
    @assert 0x1p0 == 1.0
    @assert 0x1p1 == 2.0
    @assert 0x1p2 == 4.0
    @assert 0x1p3 == 8.0
    @assert 0x1p-1 == 0.5
    @assert 0x1.8p0 == 1.5
    @assert 0x1.8p3 == 12.0
    @test (12.0) == 12.0
end

true  # Test passed
