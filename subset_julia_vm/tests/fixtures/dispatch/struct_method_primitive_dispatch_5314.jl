# Adding a Struct-arg method must not break primitive-arg dispatch (Issue #5314)
#
# Extending a base function (min/max/isless/zero/oneunit) with a Struct-typed
# method previously broke dispatch for primitive arguments. The concrete struct
# parameter `::Q5314` was misclassified: its name (an uppercase letter followed
# by digits) is read by the context-free type layer as an unbounded type
# variable, so `::Q5314` matched ANY argument. A Float64 is not a subtype of
# Q5314, so for primitive arguments the struct method must be excluded and the
# base `[Any, Any]` method selected (matching upstream Julia for a
# `Base.`-qualified extension).

using Test

struct Q5314
    I
end
Base.min(a::Q5314, b::Q5314) = Q5314(a.I)
Base.max(a::Q5314, b::Q5314) = Q5314(b.I)
Base.isless(a::Q5314, b::Q5314) = a.I < b.I
Base.zero(a::Q5314) = Q5314(0)
Base.oneunit(a::Q5314) = Q5314(1)

@testset "struct method addition keeps primitive dispatch (Issue #5314)" begin
    # Primitive arguments reach the base methods (no AmbiguousMethod / mis-dispatch).
    @test min(1.0, 2.0) == 1.0
    @test max(1.0, 2.0) == 2.0
    @test isless(1.0, 2.0)
    @test zero(3.0) == 0.0
    @test oneunit(3.0) == 1.0
    @test min(3, 7) == 3

    # Struct arguments still use the user-defined methods.
    @test min(Q5314(5), Q5314(2)).I == 5
    @test max(Q5314(5), Q5314(2)).I == 2
    @test isless(Q5314(1), Q5314(2))
    @test zero(Q5314(9)).I == 0
    @test oneunit(Q5314(9)).I == 1
end

true
