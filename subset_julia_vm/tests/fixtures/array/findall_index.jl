# findall: check second index value
# [1,2,1,3,1], findall(x -> x == 1.0, b) = [1,3,5], indices[2] = 3 (Int64)

using Test

@testset "findall: second index value (returns Int64)" begin
    b = [1.0, 2.0, 1.0, 3.0, 1.0]
    indices = findall(x -> x == 1.0, b)
    @test (indices[2]) == 3
end

true  # Test passed
