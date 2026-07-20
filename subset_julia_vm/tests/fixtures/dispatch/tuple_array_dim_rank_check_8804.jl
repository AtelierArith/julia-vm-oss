using Test

# Regression test for Issue #8804:
# Tuple{Vector{Int64}} <: Tuple{Matrix} was incorrectly true because
# array_family_pattern_params_match_with_lookup had an early return for
# pattern_params.is_empty() that bypassed the rank check.
# After fix: bare array family names (Vector, Matrix) carry their implied rank
# and a Vector (rank 1) does NOT match Matrix (rank 2).

function f(::Tuple{Vector{Int64}})
    return "vector tuple"
end

function f(::Tuple{Matrix{Int64}})
    return "matrix tuple"
end

# Vector arg should pick the vector method, not the matrix one
x = ([1, 2, 3],)
@test f(x) == "vector tuple"

# A matrix tuple should still pick the matrix method
y = ([1 2; 3 4],)
@test f(y) == "matrix tuple"

# Subtype check: Vector is not a Matrix even bare
@test !(Tuple{Vector{Int64}} <: Tuple{Matrix})
@test !(Tuple{Matrix{Int64}} <: Tuple{Vector})
@test (Tuple{Vector{Int64}} <: Tuple{Vector})
@test (Tuple{Matrix{Int64}} <: Tuple{Matrix})

true
