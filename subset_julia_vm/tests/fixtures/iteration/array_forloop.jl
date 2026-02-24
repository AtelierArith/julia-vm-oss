# Test: for loop over Array using iterate protocol

using Test

@testset "for loop over Array using iterate protocol" begin
    arr = [1, 2, 3, 4, 5]
    total = 0
    for x in arr
        total += x
    end
    @test (total) == 15.0
end

true  # Test passed
