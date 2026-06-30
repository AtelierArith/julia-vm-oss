# Issue #5051: lower bound (T>:Lower) and double bound (Lower<:T<:Upper) in
# where clauses must parse and be enforced during dispatch.
#
# Verified against upstream Julia:
# - Covariant `x::T`: only the upper bound gates matching; the lower bound is
#   absorbed into a widened `T`, so it never rejects a covariant argument.
# - Invariant `Type{T}`: `T` is bound to the argument exactly, so both bounds
#   are enforced (Lower <: T <: Upper).

using Test

# Covariant position with the double bound Integer<:T<:Real (no fallback).
# Float64 is NOT a supertype of Integer, yet it still matches covariantly
# because Julia widens T rather than enforcing the lower bound here.
gcov(x::T) where {Integer<:T<:Real} = "matched"

# Invariant Type{T} position with the double bound Integer<:T<:Real.
ginv(::Type) = "fallback"
ginv(::Type{T}) where {Integer<:T<:Real} = "matched"

# Invariant Type{T} position with a lower bound only: T>:Integer.
glb(::Type) = "fallback"
glb(::Type{T}) where {T>:Integer} = "matched"

@testset "double-bounded type variable Integer<:T<:Real (Issue #5051)" begin
    # Covariant x::T: only the upper bound restricts; the lower bound is
    # absorbed into a widened T, so a non-supertype-of-Integer arg still matches.
    @test gcov(3) == "matched"
    @test gcov(3.0) == "matched"

    # Invariant Type{T}: both bounds enforced.
    @test ginv(Integer) == "matched"
    @test ginv(Real) == "matched"
    @test ginv(Int) == "fallback"      # Integer <: Int is false
    @test ginv(Float64) == "fallback"  # Integer <: Float64 is false
    @test ginv(Number) == "fallback"   # Number <: Real is false
    @test ginv(String) == "fallback"

    # Invariant Type{T} with lower bound only.
    @test glb(Integer) == "matched"
    @test glb(Real) == "matched"       # Integer <: Real
    @test glb(Number) == "matched"     # Integer <: Number
    @test glb(Int) == "fallback"       # Integer <: Int is false
end

true
