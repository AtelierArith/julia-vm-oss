# Test Vector{Complex{Float64}} creation and indexing

using Test

@testset "Vector{Complex{Float64}} creation and indexing" begin
    arr = [Complex(1.0, 2.0), Complex(3.0, 4.0)]
    @test (real(arr[1])) == 1.0
end

true  # Test passed
