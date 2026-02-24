using Test

# Test instantiate function (Issue #2539)

@testset "instantiate" begin
    # instantiate with no axes: should compute axes
    bc = Broadcasted(+, ([1, 2, 3], [4, 5, 6]))
    ibc = instantiate(bc)
    @test ibc.axes_val !== nothing
    @test length(ibc.axes_val) == 1
    @test length(ibc.axes_val[1]) == 3

    # instantiate with pre-set axes: should validate and return as-is
    bc2 = Broadcasted(nothing, +, ([1, 2, 3], [4, 5, 6]), (1:3,))
    ibc2 = instantiate(bc2)
    @test ibc2.axes_val == (1:3,)

    # instantiate on non-Broadcasted: pass through
    @test instantiate(42) == 42
    @test instantiate([1, 2, 3]) == [1, 2, 3]
end

true
