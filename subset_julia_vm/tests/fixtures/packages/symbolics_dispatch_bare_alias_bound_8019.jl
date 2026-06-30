# Symbolics subset: a parametric bound written with the BARE imported type-alias
# `Num` (brought into scope by `using Symbolics`, `Num === Symbolics.Num`) must
# dispatch against an actual whose element type renders fully-qualified,
# `Matrix{Symbolics.Num}` (Issue #8019).
#
# Before the fix, `f(::AbstractMatrix{<:Num})` silently failed to match a
# `Matrix{Symbolics.Num}` argument (MethodError) while the equivalent qualified
# spelling `g(::AbstractMatrix{<:Symbolics.Num})` matched — a bare-vs-qualified
# `Named` normalization mismatch in the runtime dispatch resolver's bound check
# (the `(Named, Named)` arm of the core subtype engine did not treat names that
# differ only by module qualification as the same type). Upstream julia matches
# BOTH spellings because they name the same type. These assertions pass
# identically under upstream julia (verified with a local-module stand-in whose
# exported struct renders module-qualified, mirroring `Symbolics.Num`).

using Test
using Symbolics

@testset "Symbolics bare-alias parametric bound dispatch" begin
    @variables x
    A = [x x; x x]                                   # typeof: Matrix{Symbolics.Num}

    f(::AbstractMatrix{<:Num}) = "bare-Num matched"
    g(::AbstractMatrix{<:Symbolics.Num}) = "qualified matched"

    # Both the bare and the qualified bound name the same type and must match.
    @test f(A) == "bare-Num matched"
    @test g(A) == "qualified matched"

    # Arrays are invariant in their element type: a Float64 matrix is NOT a
    # `Matrix{<:Num}`, so the `{<:Num}` method must still reject it.
    @test_throws MethodError f([1.0 2.0; 3.0 4.0])
end

true
