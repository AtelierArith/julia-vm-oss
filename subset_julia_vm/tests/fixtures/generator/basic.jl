# Basic generator expression (currently uses eager evaluation)
# collect(x^2 for x in 1:5) = [1, 4, 9, 16, 25]

using Test

@testset "Basic generator expression with collect" begin
    g = (x^2 for x in 1:5)
    result = collect(g)
    @test (sum(result)) == 55.0
end

true  # Test passed
