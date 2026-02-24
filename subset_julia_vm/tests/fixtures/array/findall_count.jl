# findall: find all occurrences, count them
# [1,2,1,3,1], findall(x -> x == 1.0, b) = [1,3,5], length = 3

using Test

@testset "findall: count all occurrences (length returns Int64)" begin
    b = [1.0, 2.0, 1.0, 3.0, 1.0]
    indices = findall(x -> x == 1.0, b)
    @test (length(indices)) == 3
end

true  # Test passed
