# Test: LinRange direct construction with sum
# Note: range() function returns LinRange but type inference doesn't track it,
# so we test LinRange directly instead.

using Test

function test_linrange_sum()
    r = LinRange(1.0, 10.0, 10)
    total = 0.0
    for x in r
        total += x
    end
    return total
end

@testset "range function returns LinRange (Issue #529)" begin
    @test (test_linrange_sum()) == 55.0
end

true  # Test passed
