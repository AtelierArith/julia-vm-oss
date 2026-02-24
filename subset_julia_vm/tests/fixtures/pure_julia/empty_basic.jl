# Test empty function - create empty array

using Test

@testset "empty - create empty collection" begin

    arr = [1.0, 2.0, 3.0]
    e = empty(arr)
    @assert length(e) == 0

    @test (true)
end

true  # Test passed
