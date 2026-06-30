# Test: Issue #5257 — `Nothing <: T` for unrelated concrete `T` must be false.
#
# `Nothing` (the type of `nothing`) is a normal concrete singleton DataType,
# NOT the bottom type `Union{}`. Only `Union{}` is `<:` everything. Previously
# the runtime `<:` path conflated the two, making `Nothing <: Int64` return
# `true`. Verified against upstream Julia 1.12.

using Test

@testset "Issue #5257: concrete-type subtype correctness" begin
    # Nothing is a concrete singleton type, NOT the bottom type.
    @test (Nothing <: Any) == true
    @test (Nothing <: Nothing) == true
    @test (Nothing <: Int64) == false
    @test (Nothing <: Union{Int64}) == false
    @test (Nothing <: Union{Int64, Float64}) == false
    @test (Nothing <: Union{Nothing, Int64}) == true

    # Only Union{} (Bottom) is a subtype of everything.
    @test (Union{} <: Int64) == true
    @test (Union{} <: Nothing) == true

    # Other concrete singletons behave the same.
    @test (Missing <: Int64) == false
    @test (Missing <: Any) == true

    # Unrelated concrete pairs.
    @test (Int64 <: Float64) == false
    @test (Int64 <: Nothing) == false
    @test (Int64 <: Real) == true
    @test (Int64 <: Number) == true
end

true  # Test passed
