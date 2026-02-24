# vcat: vertical concatenation of two 1D arrays
# [1,2,3] ++ [4,5] = [1,2,3,4,5], length = 5

using Test

@testset "vcat: vertical concatenation length (Int64)" begin
    a = [1.0, 2.0, 3.0]
    b = [4.0, 5.0]
    c = vcat(a, b)
    @test (length(c)) == 5
end

true  # Test passed
