# Nested / alias-spelled parametric parameters in the exists-right `where`
# subtype solver (Advances Issue #5047).
#
# When a parametric type's parameter is itself parametric (`Box{Box{Int}}`), the
# rendered type name carries the 64-bit word alias `Int` (not the canonical
# `Int64`) on the NESTED level. The structured CoreType subtype engine parses
# operand names with `from_julia_name`, where a bare `Int` (no dedicated arm)
# becomes an opaque `Named("Int")` that is `<:` nothing. A bound check against
# such a parameter — e.g. `Box{Box{Int}} <: (Box{Box{T}} where T<:Integer)` —
# therefore wrongly returned `false`, even though the equivalent explicit-`Int64`
# spelling worked and upstream Julia returns `true`.
#
# The subtype relation now resolves a `Named("Int")`/`Named("UInt")` to its
# concrete word primitive (`Int64`/`UInt64`) so the exists-right matcher runs the
# bound check on the primitive. This is confined to subtyping — `from_julia_name`
# keeps the `Named` spelling, so type-propagation/dispatch are unchanged.
#
# All expectations below were verified against upstream Julia 1.12.

using Test

struct Box{T}
    x::T
end

@testset "nested alias param: bounded exists-right (Issue #5047)" begin
    @test (Box{Box{Int}} <: (Box{Box{T}} where T<:Integer)) == true
    @test (Box{Box{Int}} <: (Box{Box{T}} where T<:Number)) == true
    @test (Box{Box{Int}} <: (Box{Box{T}} where T<:Real)) == true
    # element fails the bound -> rejected
    @test (Box{Box{String}} <: (Box{Box{T}} where T<:Integer)) == false
    @test (Box{Box{Float64}} <: (Box{Box{T}} where T<:Integer)) == false
end

@testset "nested alias param: UInt word alias (Issue #5047)" begin
    @test (Box{Box{UInt}} <: (Box{Box{T}} where T<:Unsigned)) == true
    @test (Box{Box{UInt}} <: (Box{Box{T}} where T<:Integer)) == true
    @test (Box{Box{UInt}} <: (Box{Box{T}} where T<:Signed)) == false
end

@testset "alias in user where-bound clause (Issue #5047)" begin
    @test (Box{Int} <: (Box{T} where T<:Int)) == true
    @test (Box{UInt} <: (Box{T} where T<:UInt)) == true
    @test (Box{Float64} <: (Box{T} where T<:Int)) == false
end

@testset "deeper nesting + diagonal stay correct (Issue #5047)" begin
    @test (Box{Box{Box{Int}}} <: (Box{Box{Box{T}}} where T<:Integer)) == true
    @test (Box{Box{Box{String}}} <: (Box{Box{Box{T}}} where T<:Integer)) == false
end

# --- MUST STAY CORRECT: explicit-Int64 spelling, unbounded, non-where. ---
@testset "regression guard (Issue #5047)" begin
    # Explicit canonical spelling was always correct and must stay so.
    @test (Box{Box{Int64}} <: (Box{Box{T}} where T<:Integer)) == true
    # Unbounded `where T` accepts any nested element.
    @test (Box{Box{Int}} <: (Box{Box{T}} where T)) == true
    @test (Box{Box{String}} <: (Box{Box{T}} where T)) == true
    # Plain invariant / shape mismatches.
    @test (Box{Int} <: Box{Real}) == false
    @test (Box{Int} <: Box{Int}) == true
    @test (Box{Box{Int}} <: Box{Box{String}}) == false
end

true
