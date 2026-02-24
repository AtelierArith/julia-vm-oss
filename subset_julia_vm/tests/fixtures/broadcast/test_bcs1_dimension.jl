# Test _bcs1 single dimension broadcast computation (Issue #2535)
# Verifies the core dimension-pair broadcast logic.

using Test

@testset "_bcs1 dimension broadcast" begin
    # 1 stretches to match any dimension
    @test _bcs1(1, 5) == 5
    @test _bcs1(5, 1) == 5

    # Equal dimensions stay the same
    @test _bcs1(3, 3) == 3
    @test _bcs1(1, 1) == 1

    # Incompatible dimensions should throw DimensionMismatch
    threw = false
    try
        _bcs1(2, 3)
    catch e
        threw = e isa DimensionMismatch
    end
    @test threw
end

true
