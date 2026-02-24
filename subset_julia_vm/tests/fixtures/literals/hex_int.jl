# Hexadecimal integer literal test

using Test

@testset "Hexadecimal integer literals: 0xff, 0xFF, 0x10, 0xAB" begin
    @assert 0xff == 255
    @assert 0xFF == 255
    @assert 0x10 == 16
    @assert 0xAB == 171
    @assert 0xff_ff == 65535
    @test (1.0) == 1.0
end

true  # Test passed
