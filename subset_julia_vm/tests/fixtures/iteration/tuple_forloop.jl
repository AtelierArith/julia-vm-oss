# Test: for loop over Tuple using iterate protocol

using Test

@testset "for loop over Tuple using iterate protocol" begin
    t = (10, 20, 30)
    total = 0
    for x in t
        total += x
    end
    @test (total) == 60.0
end

true  # Test passed
