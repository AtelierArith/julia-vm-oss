# A bare `Bottom` identifier must raise `UndefVarError`, matching upstream
# Julia (Issue #10304). Upstream defines `const Bottom = Union{}` in Base
# WITHOUT exporting it, so unqualified `Bottom` is undefined in Main; only
# the canonical `Union{}` spelling (and a user's own `const Bottom = ...`,
# covered by types_agg_misc_10238.jl) names the empty type.
#
# Root cause in sjulia: (a) the prelude's `base/essentials.jl` mirrored the
# upstream const, and the flat, unqualified type-alias table leaked it into
# user scope (the general non-exported-alias leak is Issue #10578); (b) the
# static type-name parser (`JuliaType::from_name`) and several compile-side
# match arms accepted the spelling `"Bottom"` as an alias for `Union{}`.
#
# NOTE: this fixture must NOT define `const Bottom = ...` anywhere — type
# aliases are prescanned program-wide at lowering time, so a later const
# would resolve the earlier "undefined" references.
#
# All expectations below were verified against upstream Julia 1.12.

using Test

@testset "bare Bottom raises UndefVarError (Issue #10304)" begin
    # Value position (the MWE was `println(Bottom)`).
    @test_throws UndefVarError Bottom
    # `isa` right-hand side.
    @test_throws UndefVarError (1 isa Bottom)
    # Subtype-operator operands, both sides.
    @test_throws UndefVarError (Int <: Bottom)
    @test_throws UndefVarError (Bottom <: Int)
end

@testset "Union{} still carries the Bottom semantics (Issue #10304)" begin
    @test string(Union{}) == "Union{}"
    # Bottom of the lattice: subtype of every type.
    @test Union{} <: Int
    @test Union{} <: Number
    @test Union{} <: String
    @test Union{} <: Any
    @test Union{} <: Union{}
    # ... and nothing but itself is a subtype of it.
    @test !(Int <: Union{})
    @test !(Any <: Union{})
    # Zero element of typeintersect.
    @test typeintersect(Int, String) === Union{}
    @test typeintersect(Int, Union{}) === Union{}
    # Identity element of Union normalization.
    @test Union{Union{}, Int} === Int
end

true
