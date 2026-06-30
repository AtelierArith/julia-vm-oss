# Issue #5650: type display must carry where-clause LOWER bounds and normalize the
# contravariant `>:` shorthand.
#
# Previously `JuliaType::UnionAll` carried only a single (upper) bound, and the
# value-position `where` parser flattened `>:` / `Lower<:T<:Upper` constraints
# into a generic binary expression that dropped the lower bound; `Vector{>:Int}`
# also survived as an unnormalized raw string.

using Test

# where-clause lower bounds (single, double, and the existing upper-only form).
@test string(Vector{T} where Int<:T<:Real) == "Vector{T} where Int64<:T<:Real"
@test string(Vector{T} where T>:Int) == "Vector{T} where T>:Int64"
@test string(Vector{T} where T<:Real) == "Vector{T} where T<:Real"
@test string(Array{T} where Int8<:T<:Signed) == "Array{T} where Int8<:T<:Signed"

# Anonymous contravariant shorthand `>:Bound` with alias normalization (Int->Int64).
@test string(Vector{>:Int}) == "Vector{>:Int64}"
@test string(Vector{>:Integer}) == "Vector{>:Integer}"

# Regression: the covariant `<:Bound` shorthand and unbounded/concrete parametric
# types still render unchanged.
@test string(Vector{<:Real}) == "Vector{<:Real}"
@test string(Vector{Int}) == "Vector{Int64}"

true
