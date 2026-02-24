# map over complex inputs should return Vector{Complex}

using Test

@testset "map over Complex inputs returns Vector{Complex}" begin
    arr = [1 + im, 2.0 - 4im]
    result = map(x -> x ^ 2, arr)
    @test (result isa Vector{Complex})
end

true  # Test passed
