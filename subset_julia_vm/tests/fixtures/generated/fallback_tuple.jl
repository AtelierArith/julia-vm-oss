# Test: @generated fallback with sum of squares
# Simulates ntuple-like behavior

using Test

function make_squares(n::Int)
    result = 0
    if @generated
        # Would generate optimized code in full Julia
        result = -1
    else
        # Fallback: use loop to compute sum of squares
        total = 0
        for i in 1:n
            total = total + i^2
        end
        result = total
    end
    result
end

@testset "@generated with sum of squares fallback" begin


    @assert make_squares(3) == 14  # 1 + 4 + 9
    @assert make_squares(4) == 30  # 1 + 4 + 9 + 16

    # Return true if all tests passed
    @test (true)
end

true  # Test passed
