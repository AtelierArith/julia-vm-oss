# Test: LinRange getindex via iteration

using Test

function test_linrange_indexing()
    r = LinRange(0.0, 1.0, 5)
    # Elements should be: 0.0, 0.25, 0.5, 0.75, 1.0
    # Test first and last elements via iteration
    first_val = 0.0
    last_val = 0.0
    for (i, x) in enumerate(r)
        if i == 1
            first_val = x
        end
        last_val = x
    end
    return first_val + last_val
end

@testset "LinRange first and last element (Issue #529)" begin
    @test (test_linrange_indexing()) == 1.0
end

true  # Test passed
