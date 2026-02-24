# Test show/display methods for broadcast types (Issue #2551)
# These test SubsetJuliaVM-specific non-parametric workaround types.

using Test

@testset "show BroadcastStyle types" begin
    # DefaultArrayStyle with dims field
    s1 = DefaultArrayStyle(1)
    @test repr(s1) == "DefaultArrayStyle{1}()"

    s2 = DefaultArrayStyle(2)
    @test repr(s2) == "DefaultArrayStyle{2}()"

    s0 = DefaultArrayStyle(0)
    @test repr(s0) == "DefaultArrayStyle{0}()"

    # TupleBroadcastStyle
    ts = TupleBroadcastStyle()
    @test repr(ts) == "Style{Tuple}()"

    # BroadcastStyleUnknown
    us = BroadcastStyleUnknown()
    @test repr(us) == "Unknown()"
end

@testset "show Broadcasted" begin
    # Basic Broadcasted with + function and scalar args
    bc = Broadcasted(nothing, +, (1, 2))
    r = repr(bc)
    @test occursin("Broadcasted(", r)
    @test occursin("+", r)
    @test occursin("1", r)
    @test occursin("2", r)
end

true
