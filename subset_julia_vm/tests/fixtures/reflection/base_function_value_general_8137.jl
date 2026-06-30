# Ordinary Base functions resolve as callable function values via qualified
# `Base.<name>` access (Issue #8137). Continuation of the Base.<fn> value-lookup
# series (#4960-#4966 / umbrella #4119): the earlier work covered specific
# reflection/conversion helpers; this covers the general case — now-Pure-Julia
# Base functions (`map`, `filter`, `sin`, `cos`, `reduce`, `foldl`, `sum`, …)
# that are not in the `is_base_function` allowlist but ARE backed by a method
# table, so `f = Base.map` must produce the same callable as the unqualified
# `map`. Upstream Julia resolves `Base.map` to the function object.
using Test

@testset "Base higher-order function values" begin
    f = Base.map
    @test f isa Function
    @test f(x -> x + 1, [1, 2, 3]) == [2, 3, 4]

    g = Base.filter
    @test g isa Function
    @test g(iseven, [1, 2, 3, 4]) == [2, 4]

    r = Base.reduce
    @test r isa Function
    @test r(+, [1, 2, 3, 4]) == 10

    fl = Base.foldl
    @test fl isa Function
    @test fl(+, [1, 2, 3, 4]) == 10

    s = Base.sum
    @test s isa Function
    @test s([1, 2, 3]) == 6
end

@testset "Base math function values" begin
    sn = Base.sin
    @test sn isa Function
    @test sn(0.0) == 0.0

    cs = Base.cos
    @test cs isa Function
    @test cs(0.0) == 1.0
end

@testset "qualified Base.<fn> ignores a same-named local shadow" begin
    # Qualified `Base.map` must resolve to the Base function regardless of a
    # same-named local binding — qualified access bypasses local shadowing.
    map = 99
    h = Base.map
    @test h isa Function
    @test h(x -> x * 2, [1, 2, 3]) == [2, 4, 6]
    @test map == 99
end

true
