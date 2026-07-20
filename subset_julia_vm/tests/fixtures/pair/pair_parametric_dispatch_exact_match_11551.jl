# Issue #11551: multiple Pair{A,B} parametric methods must dispatch to the
# EXACT runtime match, not report ambiguity. `Pair` is modeled as a
# non-parametric struct at the value level in sjulia (`struct Pair; first;
# second; end`, base/pair.jl), so the dispatch-facing runtime type projection
# must reconstruct `Pair{typeof(first), typeof(second)}` (the same projection
# `typeof`/`isa` already use per Issue #10577) for two-parameter Pair method
# signatures to resolve distinctly instead of comparing an identical bare
# `Pair` runtime type against both candidates.
#
# Verified against upstream `julia --startup-file=no` (1.12.6): prints `1`
# then `2`.
using Test

f11551(x::Pair{Int64,Int64}) = 1
f11551(x::Pair{Int64,Float64}) = 2

@testset "Pair{A,B} parametric methods dispatch to the exact match (Issue #11551)" begin
    @test f11551(Pair(1, 2)) == 1
    @test f11551(Pair(1, 2.0)) == 2
end

true
