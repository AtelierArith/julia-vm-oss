# Issue #5383 (sub-case 2): a value-position type variable whose upper bound is
# itself a parametric type mentioning another `where` variable —
# `fv(x::T) where {S<:Number, T<:Vector{S}}` — must match a `Vector{<:Number}`
# argument and reject other vectors. Previously `fv([1])` fell through to the
# untyped `fv(x)` fallback (`:any`) because the bound `Vector{S}` was converted
# opaquely via `from_julia_name`, which dropped the `S<:Number` constraint and
# compared the concrete element `Int64` against the bare type variable `S`,
# rejecting the match entirely.
#
# The covariant single-use parameter `x::T` is equivalent to `x::Vector{S}`, so
# the fix matches the argument structurally against the parsed bound pattern
# (binding `S` and enforcing `S<:Number`) — the same path that already handles
# the direct `x::Vector{S}` spelling.

using Test

fv(x) = :any
fv(x::T) where {S<:Number, T<:Vector{S}} = :vecnum

@testset "parametric typevar bound value dispatch (Issue #5383)" begin
    @test fv([1, 2]) == :vecnum        # Vector{Int64}: Int64 <: Number
    @test fv([1.0]) == :vecnum          # Vector{Float64}: Float64 <: Number
    @test fv(["a", "b"]) == :any        # Vector{String}: String is NOT <: Number
    @test fv(5) == :any                 # not a Vector at all
end

true
