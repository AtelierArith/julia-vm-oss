# Binary integer literal test

using Test

@testset "Binary integer literals: 0b0, 0b1, 0b1010, 0B1010" begin
    @assert 0b0 == 0
    @assert 0b1 == 1
    @assert 0b10 == 2
    @assert 0b1010 == 10
    @assert 0b11111111 == 255
    @assert 0b1111_0000 == 240
    @test (1.0) == 1.0
end

true  # Test passed
