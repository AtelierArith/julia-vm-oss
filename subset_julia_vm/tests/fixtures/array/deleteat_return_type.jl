# Test that deleteat! returns the mutated array, not the removed element (Issue #3468)
# Julia: typeof(deleteat!([1,2,3], 2)) == Vector{Int64}

using Test

@testset "array_deleteat_return_type: deleteat! returns mutated array" begin
    a = [1, 2, 3]
    result = deleteat!(a, 2)
    @test typeof(result) == Vector{Int64}
    @test result === a
    @test length(result) == 2
    @test result[1] == 1
    @test result[2] == 3
end

true
