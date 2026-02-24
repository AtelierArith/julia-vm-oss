# map with mixed numeric inputs should return Vector{Float64}

using Test

@testset "map over mixed numeric inputs returns Vector{Float64}" begin
    arr = [1.0, 2, 3]
    result = map(x -> x ^ 2, arr)
    @test (typeof(result) === Vector{Float64})
end

true  # Test passed
