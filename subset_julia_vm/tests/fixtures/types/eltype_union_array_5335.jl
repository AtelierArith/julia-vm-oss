# eltype of a Union-typed array must be a Union type object (Issue #5335)
#
# Previously the VM materialized the element type from an array's UnionOf tag as
# a `DataType`-tagged Struct("Union{...}") name rather than a real Union type
# object, so `eltype(v) == Union{...}` was false and `typeof(eltype(v))` was
# DataType instead of Union.

using Test

@testset "eltype of Union-typed array is a Union (Issue #5335)" begin
    v = Union{Int64,Float64}[1]
    @test eltype(v) == Union{Int64,Float64}
    @test string(typeof(eltype(v))) == "Union"
    # Union membership is order-independent.
    @test eltype(v) == Union{Float64,Int64}
    # Bare literal comparison (sanity, already worked).
    @test Union{Int64,Float64} == Union{Int64,Float64}
    # typeof of a bare union literal is also `Union`.
    @test string(typeof(Union{Int64,Float64})) == "Union"
    # A three-member union round-trips too.
    w = Union{Int64,Float64,String}[1]
    @test eltype(w) == Union{Int64,Float64,String}
    @test string(typeof(eltype(w))) == "Union"
end

# Return true to indicate success.
true
