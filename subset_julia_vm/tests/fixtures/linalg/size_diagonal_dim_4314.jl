using Test
using LinearAlgebra

@testset "size Diagonal dim dispatch #4314" begin
    D = Diagonal([1.0, 2.0, 3.0])
    @test size(D, 1) == 3
    @test size(D, 2) == 3
end

true
