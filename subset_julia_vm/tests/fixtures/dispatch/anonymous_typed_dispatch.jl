# Test method dispatch for anonymous typed parameters (::StructType)
# Issue #635: Method dispatch fails to distinguish struct subtypes

using Test

# Define abstract type hierarchy
abstract type AbstractIrrational <: Real end

# Define concrete struct types
struct IrrationalPi <: AbstractIrrational end
struct IrrationalCatalan <: AbstractIrrational end

# Define methods with anonymous typed parameters
# The :: syntax without parameter name should correctly parse the type
f(::IrrationalPi) = 1
f(::IrrationalCatalan) = 2

# Test that dispatch works correctly
@test f(IrrationalPi()) == 1
@test f(IrrationalCatalan()) == 2

# Test with multiple concrete types
struct PointA end
struct PointB end

g(::PointA) = "A"
g(::PointB) = "B"

@test g(PointA()) == "A"
@test g(PointB()) == "B"

# Test mixed named and anonymous parameters
h(x::Int64, ::PointA) = x + 1
h(x::Int64, ::PointB) = x + 2

@test h(10, PointA()) == 11
@test h(10, PointB()) == 12

# Return true to indicate success
true
