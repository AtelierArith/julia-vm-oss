using Test

# Issue #5674: mutating builtins (insert!/deleteat!/pushfirst!/push!) rejected a
# non-variable (literal) array first argument with "first argument must be a
# variable". The mutation instructions leave the modified array on the stack, so a
# literal array value can be mutated and returned directly (no binding to store back).

@testset "mutating builtins on a literal array (Issue #5674)" begin
    @test insert!([1, 2, 3], 2, 99) == [1, 99, 2, 3]
    @test insert!([10, 20, 30], 1, 0) == [0, 10, 20, 30]
    @test deleteat!([1, 2, 3, 4], 2) == [1, 3, 4]
    @test deleteat!([1, 2, 3], 3) == [1, 2]
    @test pushfirst!([2, 3], 1) == [1, 2, 3]
    @test push!([1, 2], 3) == [1, 2, 3]
    @test push!(["a", "b"], "c") == ["a", "b", "c"]

    # Element-type handling matches the variable path.
    @test typeof(push!([1, 2], 3)[3]) == Int64        # Int array keeps Int
    @test push!([1.0, 2.0], 3) == [1.0, 2.0, 3.0]     # Float64 array widens
    @test typeof(push!(Float64[1, 2], 3)[3]) == Float64

    # The variable path is unchanged (regression).
    v = [1, 2, 3]
    insert!(v, 2, 99)
    @test v == [1, 99, 2, 3]
    push!(v, 7)
    @test v == [1, 99, 2, 3, 7]
end

true
