# Issue #10583: a typed-empty-array-literal head naming a genuinely undefined
# identifier (`SomeUndefName[]`) silently compiled to `Any[]`. Upstream lowers
# `T[]` to `getindex(T)` after resolving `T` as an ordinary global, so an
# undefined head raises UndefVarError. Identifier-shaped unknown heads now
# route through `getindex(head)`; resolvable heads keep their semantics.

using Test

@testset "undefined typed-empty-array head raises (Issue #10583)" begin
    @test_throws UndefVarError SomeUndefNameQQ10583[]
    # Value bindings keep getindex routing (Issue #6839 behavior unchanged).
    LOG = Ref(7)
    @test LOG[] == 7
    T = Int
    @test typeof(T[]) === Vector{Int64}
    # Known type heads unchanged.
    @test typeof(Int32[]) === Vector{Int32}
    @test typeof(Union{Int,String}[]) === Vector{Union{Int64, String}}
end

true
