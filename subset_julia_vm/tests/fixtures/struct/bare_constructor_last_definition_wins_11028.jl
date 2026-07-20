using Test

include("bare_constructor_last_definition_wins_11028_inner_helper.jl")
include("bare_constructor_last_definition_wins_11028_later_outer_helper.jl")
include("bare_constructor_last_definition_wins_11028_outer_helper.jl")
include("bare_constructor_last_definition_wins_11028_later_inner_helper.jl")

struct BareInnerThenOuter11028
    x::Int
    BareInnerThenOuter11028(x::Int) = new(x + 1)
end
BareInnerThenOuter11028(x::Int) = :outer

BareOuterThenInner11028(x::Int) = :outer
struct BareOuterThenInner11028
    x::Int
    BareOuterThenInner11028(x::Int) = new(x + 1)
end

@testset "bare constructor last-definition-wins (Issue #11028)" begin
    @test BareInnerThenOuter11028(10) === :outer

    reverse = BareOuterThenInner11028(10)
    @test reverse isa BareOuterThenInner11028
    @test reverse.x == 11

    @test IncludedInnerThenOuter11028(10) === :outer

    included_reverse = IncludedOuterThenInner11028(10)
    @test included_reverse isa IncludedOuterThenInner11028
    @test included_reverse.x == 11
end

true
