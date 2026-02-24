# vcat: check element value at position 4
# [1,2,3] ++ [4,5] = [1,2,3,4,5], c[4] = 4.0

using Test

@testset "vcat: element value check" begin
    a = [1.0, 2.0, 3.0]
    b = [4.0, 5.0]
    c = vcat(a, b)
    @test (c[4]) == 4.0
end

true  # Test passed
