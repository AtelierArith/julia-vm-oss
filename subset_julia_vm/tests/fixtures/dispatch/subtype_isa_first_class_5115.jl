# Issue #5115: <: / >: / isa as first-class function/operator values
# Upstream Julia treats <:, >:, isa as ordinary functions (julia/base/operators.jl).
# They must be usable as referenceable callables: (<:)(A, B), bound to a variable,
# qualified as Base.:(<:), and used inside higher-order predicates.
using Test

# --- 2-arg calls as first-class function values ---
@test (<:)(Int, Number) == true
@test (<:)(Number, Int) == false
@test (>:)(Number, Int) == true
@test (>:)(Int, Number) == false
@test isa(3, Int) == true
@test isa(3, String) == false

# --- bound to a variable, then called ---
sub = (<:)
@test sub(Int, Number) == true
@test sub(String, Number) == false

sup = (>:)
@test sup(Number, Int) == true

isa_fn = isa
@test isa_fn(3, Int) == true
@test isa_fn("x", Int) == false

# --- Base.:(op) qualified references ---
@test Base.:(<:)(Int, Number) == true
@test Base.:(>:)(Number, Int) == true
@test Base.:(isa)(3, Int) == true

# --- higher-order: filter over a vector of types ---
@test filter(t -> t <: Real, [Int, String, Float64]) == [Int, Float64]
@test filter(t -> t <: Real, Any[Int, String, Float64]) == Any[Int, Float64]

# --- higher-order: map with isa ---
@test map(x -> isa(x, Int), Any[1, "a", 2.0]) == Bool[true, false, false]

println("all 5115 checks passed")
true
