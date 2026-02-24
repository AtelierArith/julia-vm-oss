using Test

# Test Broadcasted struct and basic operations (Issue #2534 workaround)

@testset "Broadcasted struct" begin
    # Create a Broadcasted with two array args
    bc = Broadcasted(+, ([1, 2, 3], [4, 5, 6]))
    @test bc.bc_args[1] == [1, 2, 3]
    @test bc.bc_args[2] == [4, 5, 6]
    @test bc.axes_val === nothing

    # Test axes computation
    ax = axes(bc)
    @test length(ax) == 1
    @test length(ax[1]) == 3

    # Test length
    @test length(bc) == 3

    # Test eachindex
    idx = eachindex(bc)
    @test first(idx) == 1
    @test last(idx) == 3

    # Test getindex (should evaluate the broadcast expression)
    @test bc[1] == 5   # 1 + 4
    @test bc[2] == 7   # 2 + 5
    @test bc[3] == 9   # 3 + 6
end

true
