# A `where` binder whose variable repeats in an earlier parameter position must
# NOT be elided from the printed parameter list: `Pair{T, T} where T` prints in
# full and stays alpha-equivalent under subtyping. Eliding it collapsed the type
# to `Pair{T}`, which broke both display and structural `<:` (the operator
# stringifies through the collapsed name). Issue #10635.

using Test

struct MyPair10635{A,B}
    a::A
    b::B
end

@testset "Pair{T,T} where T does not collapse (Issue #10635)" begin
    x = Pair{T,T} where T
    y = Pair{S,S} where S

    # Display keeps both (duplicate-named) parameters and the binder.
    @test string(x) == "Pair{T, T} where T"
    @test string(y) == "Pair{S, S} where S"

    # Alpha-equivalent same-name binders compare equal and subtype both ways.
    @test x == y
    @test x <: y
    @test y <: x
    @test x <: x

    # A binder nested inside an earlier parameter also blocks elision.
    @test string(Pair{Vector{B},B} where B) == "Pair{Vector{B}, B} where B"

    # A user-defined two-parameter family collapses the same way as `Pair`.
    mx = MyPair10635{T,T} where T
    my = MyPair10635{S,S} where S
    @test string(mx) == "MyPair10635{T, T} where T"
    @test mx <: my
    @test my <: mx

    # Positive controls: eliding a trailing binder that appears nowhere else is
    # still correct (upstream prints the shortened form).
    @test string(Pair{Int64,B} where B) == "Pair{Int64}"
    @test string(Pair{Bool,B} where B) == "Pair{Bool}"
    @test string(Pair{Int64,Vector{B}} where B) == "Pair{Int64, Vector{B}} where B"
end

true
