using Test

observed = false

@testset "first local z" begin
    z = 1.0 + 2.0im
    @test z^2 == z*z
end

@testset "second local z" begin
    z = ComplexF32(1, 2)
    global observed = typeof(z + z) == ComplexF32
    @test observed
end

observed
