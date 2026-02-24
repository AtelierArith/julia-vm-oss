# Filtered generator expression
# collect(x for x in 1:10 if x > 5) = [6, 7, 8, 9, 10]

using Test

@testset "Filtered generator expression" begin
    g = (x for x in 1:10 if x > 5)
    result = collect(g)
    @test (sum(result)) == 40.0
end

true  # Test passed
