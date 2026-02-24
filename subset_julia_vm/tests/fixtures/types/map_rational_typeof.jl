# map over Rational inputs should preserve Rational element type

using Test

@testset "map over Rational inputs preserves Rational element type" begin
    arr = [1//3, 1//3, 1//3]
    result = map(x -> x ^ 2, arr)
    @test (typeof(result) === Vector{Rational{Int64}})
end

true  # Test passed
