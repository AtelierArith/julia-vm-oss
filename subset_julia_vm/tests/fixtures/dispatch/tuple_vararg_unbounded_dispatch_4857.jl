# Test Tuple{Vararg{T}} (unbounded length, one-arg Vararg) dispatch (Issue #4857)
# Tuple{Vararg{T}} is a homogeneous tuple of any length whose elements share
# type T. The single-argument Vararg{T} carries no length parameter, so it must
# match a tuple-valued argument with N >= 0 elements and bind T to the element
# type.
using Test

# Free element type bound by where-clause.
g(xs::Tuple{Vararg{T}}) where T = T

# Concrete element type — selects on element type, any length.
h(xs::Tuple{Vararg{Int}}) = length(xs)
k(xs::Tuple{Vararg{String}}) = length(xs)

# Dispatch between two unbounded vararg-tuple methods.
f(xs::Tuple{Vararg{Int}}) = "ints"
f(xs::Tuple{Vararg{String}}) = "strings"

@testset "Tuple{Vararg{T}} unbounded dispatch" begin
    # where-clause T bound to the (shared) element type.
    @test g((1, 2, 3)) == Int64
    @test g((Int32(1),)) == Int32

    # Concrete element type, any length (including zero).
    @test h((1, 2, 3, 4)) == 4
    @test h(()) == 0
    @test k(("a", "b")) == 2

    # Dispatch selects the method whose element type matches.
    @test f((1, 2, 3)) == "ints"
    @test f(("a", "b")) == "strings"
end

true
