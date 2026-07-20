# Regression guard for Issue #10576: value-level `isa` against an anonymous
# covariant bound (`Vector{<:AniIsa}`) naming a USER abstract type disagreed
# with `typeof(x) <: T`. `[DogIsa()] isa Vector{<:AniIsa}` was `false` while
# `Vector{DogIsa} <: Vector{<:AniIsa}` and `typeof([DogIsa()]) <: Vector{<:AniIsa}`
# were both `true`. Builtin bounds (`Vector{<:Real}`) already worked, so the gap
# was specific to runtime-declared bound names on the array `isa` path, which
# fell back to a static `type_values_subtype` that did not know the user
# abstract-type hierarchy. The fix routes every array carrier (StructRef/Struct
# wrapper and the legacy native carrier) through the runtime `check_subtype`
# (the same engine behind `typeof(x) <: T`), restoring upstream's invariant
# `x isa T` ⟺ `typeof(x) <: T`.
#
# NOTE: assertions use `@test`, not `@assert` — `@assert` wrapping an expression
# that syntactically contains a `<:Bound` shorthand trips a separate, unrelated
# lowering bug (`<:` mis-lowered as `<` on `Type{Bound}`), independent of this fix.

using Test

abstract type AniIsa end
abstract type MammalIsa <: AniIsa end
struct DogIsa <: MammalIsa end
struct CatIsa <: MammalIsa end
struct FishIsa <: AniIsa end

@testset "value-level isa vs anonymous covariant bound naming a user abstract type (Issue #10576)" begin
    # The exact MWE from the issue.
    @test [1.0] isa Vector{<:Real}                    # builtin bound: stays true
    @test [DogIsa()] isa Vector{<:AniIsa}             # user abstract bound: was WRONG (false)
    @test Vector{DogIsa} <: Vector{<:AniIsa}
    @test typeof([DogIsa()]) <: Vector{<:AniIsa}

    # Invariant user-struct element and covariant chains.
    @test [DogIsa()] isa Vector{DogIsa}
    @test [DogIsa()] isa Vector{<:MammalIsa}
    @test !([DogIsa()] isa Vector{MammalIsa})         # invariant: DogIsa != MammalIsa
    @test !([DogIsa()] isa Vector{CatIsa})
    @test !([DogIsa()] isa Vector{AniIsa})            # invariant

    # Abstract-container covariant checks. (An anonymous covariant bound
    # combined with an explicit ndims value parameter — `AbstractArray{<:AniIsa,1}`
    # — additionally depends on a `check_subtype` capability that is a separate
    # in-progress milestone-76 gap, so it is exercised only through the
    # isa ⟺ typeof(x)<:T agreement assertion below, not as an absolute value.)
    @test [DogIsa()] isa AbstractVector{<:AniIsa}
    @test [DogIsa()] isa AbstractArray{<:AniIsa}
    @test [DogIsa()] isa Vector
    @test [DogIsa()] isa Array
    @test [DogIsa()] isa Any
    @test !([DogIsa()] isa Vector{Int})
    @test !([DogIsa()] isa AniIsa)                    # the array is not the element

    # Matrices route through the same array path.
    @test [DogIsa() DogIsa()] isa Matrix{<:AniIsa}
    @test [DogIsa() DogIsa()] isa Matrix{DogIsa}
    @test [DogIsa() DogIsa()] isa AbstractMatrix{<:AniIsa}
    @test !([DogIsa() DogIsa()] isa Vector{<:AniIsa})

    # Union targets and empty typed arrays.
    @test [DogIsa()] isa Union{Vector{DogIsa},Vector{CatIsa}}
    @test !([FishIsa()] isa Union{Vector{DogIsa},Vector{CatIsa}})
    @test DogIsa[] isa Vector{<:AniIsa}
    @test DogIsa[] isa Vector{DogIsa}

    # Builtin element bounds must keep working after the reroute.
    @test [1, 2, 3] isa Vector{<:Real}
    @test [1, 2, 3] isa Vector{<:Integer}
    @test !([1.0, 2.0] isa Vector{<:Integer})

    # The core invariant `x isa T` ⟺ `typeof(x) <: T`.
    @test ([DogIsa()] isa Vector{<:AniIsa}) == (typeof([DogIsa()]) <: Vector{<:AniIsa})
    @test ([DogIsa()] isa Vector{DogIsa}) == (typeof([DogIsa()]) <: Vector{DogIsa})
    @test ([DogIsa()] isa Vector{MammalIsa}) == (typeof([DogIsa()]) <: Vector{MammalIsa})
    @test ([DogIsa()] isa AbstractVector{<:AniIsa}) == (typeof([DogIsa()]) <: AbstractVector{<:AniIsa})
    @test ([DogIsa()] isa AbstractArray{<:AniIsa,1}) == (typeof([DogIsa()]) <: AbstractArray{<:AniIsa,1})
    @test ([1.0] isa Vector{<:Real}) == (typeof([1.0]) <: Vector{<:Real})
    @test (DogIsa[] isa Vector{<:AniIsa}) == (typeof(DogIsa[]) <: Vector{<:AniIsa})
end

true
