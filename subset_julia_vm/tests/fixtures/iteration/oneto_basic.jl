# Test OneTo - AbstractUnitRange that behaves like 1:n
# OneTo(n) represents a range that behaves like 1:n
# Based on Julia's base/range.jl:470-492

using Test

@testset "OneTo basic (Issue #490)" begin
    # Create OneTo(5) which represents 1:5
    r = OneTo(5)

    # Test iteration - for loop over OneTo
    total = 0
    count = 0
    for x in r
        total = total + x
        count = count + 1
    end
    @test (total == 15)  # 1 + 2 + 3 + 4 + 5 = 15
    @test (count == 5)   # 5 elements
end

true  # Test passed
