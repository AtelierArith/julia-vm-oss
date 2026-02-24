# findlast: find last occurrence of value
# [1,2,3,2,4], findlast(2.0) = 4

using Test

@testset "findlast: find last occurrence (returns Int64 index)" begin
    a = [1.0, 2.0, 3.0, 2.0, 4.0]
    @test (findlast(2.0, a)) == 4
end

true  # Test passed
