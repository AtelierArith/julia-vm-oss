using Test

# Test materialize and materialize! functions (Issue #2540)

@testset "materialize" begin
    # materialize: Broadcasted → Array
    bc = Broadcasted(+, ([1, 2, 3], [4, 5, 6]))
    result = materialize(bc)
    @test length(result) == 3
    @test result[1] == 5
    @test result[2] == 7
    @test result[3] == 9

    # materialize on non-Broadcasted: pass through
    @test materialize(42) == 42
    @test materialize([1, 2]) == [1, 2]
end

@testset "materialize!" begin
    # materialize! with Broadcasted source
    bc = Broadcasted(*, ([2, 3, 4], [10, 20, 30]))
    dest = zeros(Int64, 3)
    materialize!(dest, bc)
    @test dest[1] == 20
    @test dest[2] == 60
    @test dest[3] == 120
end

true
