# Regression test: similar(::Broadcasted, ::Type) must NOT be intercepted
# by the builtin similar() handler which expects similar(array, n::Int64).
# The compile-time guard in call.rs (Issue #2700, #2702) ensures that
# only Array-typed first arguments route to the builtin; Broadcasted
# arguments fall through to Pure Julia method dispatch.

using Test

@testset "similar with Broadcasted falls through to Pure Julia" begin
    # Create a Broadcasted object (lazy wrapper)
    bc = Broadcasted(nothing, +, ([1, 2, 3], [4, 5, 6]), (1:3,))

    # similar(::Broadcasted, ::Type) should dispatch to Pure Julia,
    # NOT to the Rust builtin (which would fail or produce wrong results)
    dest = similar(bc, Int64)
    @test isa(dest, Vector{Int64})
    @test length(dest) == 3

    # similar for Float64 Broadcasted
    bc_float = Broadcasted(nothing, +, ([1.0, 2.0, 3.0], [4.0, 5.0, 6.0]), (1:3,))
    dest_float = similar(bc_float, Float64)
    @test isa(dest_float, Vector{Float64})
    @test length(dest_float) == 3
end

@testset "similar with Array still uses builtin" begin
    # Regular array similar() should still work via the builtin path
    arr = [1, 2, 3]
    dest = similar(arr)
    @test length(dest) == 3
end

true
