# Test OneTo with zero (empty range)
# OneTo(0) represents an empty range like 1:0

using Test

@testset "OneTo empty (Issue #490)" begin
    # Create OneTo(0) - empty range
    r = OneTo(0)

    # Test iteration produces no elements
    count = 0
    for x in r
        count = count + 1
    end
    @test (count == 0)
end

true  # Test passed
