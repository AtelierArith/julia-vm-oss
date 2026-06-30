# Test that setindex! returns the mutated collection, not the value (Issue #3477)

using Test

@testset "array_setindex_return_type: setindex! returns mutated collection" begin
    a = [1, 2, 3]
    result = setindex!(a, 9, 1)
    @test typeof(result) == Vector{Int64}
    @test result === a
    @test result[1] == 9
end

true
