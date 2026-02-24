# Test: for loop over UnitRange using iterate protocol

using Test

@testset "for loop over UnitRange using iterate protocol" begin
    total = 0
    for x in 1:5
        total += x
    end
    @test (total) == 15.0
end

true  # Test passed
