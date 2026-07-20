# Issue #9103: a generator expression with an arbitrary (non-`f(var)`) body
# must be LAZY — side effects fire at iteration/collect time, never at
# construction time — and `g.iter` must keep the original iterator instead of
# a precomputed array. Also covers the consumer gaps found alongside
# (Issue #9128): first/getindex applying the mapping, count(itr),
# Tuple(::Generator), map over a lazy generator (generator fusion), and
# builtin-composed calls.

using Test

@testset "side effects fire at collect time, not construction" begin
    log = String[]
    g = (begin
        push!(log, "computing $x")
        x^2
    end for x in 1:3)
    @test isempty(log)                     # construction ran nothing
    @test collect(g) == [1, 4, 9]
    @test log == ["computing 1", "computing 2", "computing 3"]
end

@testset "g.iter keeps the original iterator" begin
    r = 1:5
    g = (x^2 for x in r)
    @test g.iter == 1:5
    @test length(g) == 5
end

@testset "lazy generators inside a function body capture locals" begin
    function make_sums(a)
        collect(x^2 + a for x in 1:3)
    end
    @test make_sums(10) == [11, 14, 19]
end

@testset "consumers apply the mapping on lazy generators" begin
    @test collect(x^2 for x in 2:4) == [4, 9, 16]
    @test sum(x^2 for x in 2:4) == 29
    @test first(x^2 for x in 2:4) == 4
    @test maximum(x^2 for x in 2:4) == 16
    @test minimum(x - 5 for x in 2:4) == -3
    @test prod(x + 1 for x in 1:3) == 24
    @test join((x^2 for x in 1:3), ",") == "1,4,9"
    @test any(x > 2 for x in 1:3)
    @test !all(x > 2 for x in 1:3)
    @test count(x % 2 == 0 for x in 1:4) == 2
    @test Tuple(x^2 for x in 1:3) == (1, 4, 9)
    @test [y for y in (x * 2 for x in 1:3)] == [2, 4, 6]
    @test collect(s * "!" for s in ["a", "b"]) == ["a!", "b!"]
    @test collect(a + b for (a, b) in zip(1:3, 4:6)) == [5, 7, 9]
end

@testset "map / reduce over a lazy generator (generator fusion)" begin
    g = (x^2 for x in 1:3)
    @test map(y -> y + 1, g) == [2, 5, 10]
    @test map(string, (x^2 for x in 1:3)) == ["1", "4", "9"]
    @test reduce(+, (x^2 for x in 1:3)) == 14
    @test sum(sum(y for y in 1:x) for x in 1:3) == 10
end

@testset "iterate protocol drives lazy bodies" begin
    g = (x^2 for x in 2:4)
    y = iterate(g)
    @test y[1] == 4
    y2 = iterate(g, y[2])
    @test y2[1] == 9
    out = Int[]
    for v in (x + 10 for x in 1:3)
        push!(out, v)
    end
    @test out == [11, 12, 13]
end

@testset "builtin inside a composed call (Issue #9128)" begin
    @test (string ∘ abs)(-2) == "2"
end

@testset "filtered generators collect correctly (now lazy, Issue #9127)" begin
    # Filtered non-trivial bodies are now LAZY too (Issue #9127); values must
    # still be correct. Ordering/laziness is asserted in
    # filtered_generator_lazy_9127.jl.
    @test collect(x^2 for x in 1:4 if x % 2 == 0) == [4, 16]
    @test sum(x^2 for x in 1:4 if x > 1) == 29
end

true
