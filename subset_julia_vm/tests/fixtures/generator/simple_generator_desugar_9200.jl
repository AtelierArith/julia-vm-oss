# Issue #9200 (S2): the SIMPLE generator form `(f(x) for x in iter)` — single
# scalar binding, unfiltered — is desugared in lowering to the upstream
# `Base.Generator(func, iter)` shape (julia-syntax.scm `expand-generator` /
# `func-for-generator-ranges`): `body === var` uses `identity`, otherwise the
# body is lifted into an anonymous function. This must preserve every observable
# behavior of the previous MakeGenerator path, verified against upstream julia.

using Test

@testset "identity generator (body === var)" begin
    @test collect(x for x in 1:4) == [1, 2, 3, 4]
    @test sum(x for x in 1:4) == 10
    @test collect(x for x in [10, 20, 30]) == [10, 20, 30]
    @test first(x for x in 3:9) == 3
end

@testset "mapped generator over a range / array / tuple" begin
    @test collect(x * x for x in 1:4) == [1, 4, 9, 16]
    @test sum(x * x for x in 1:4) == 30
    @test collect(x * x for x in (1, 2, 3)) == [1, 4, 9]
    @test collect(x + 100 for x in [1, 4, 9]) == [101, 104, 109]
    # Float ranges preserve element type.
    @test collect(x * x for x in 0.0:0.5:1.0) == [0.0, 0.25, 1.0]
end

@testset "collect over a generator whose iterator is itself a generator" begin
    # Generator fusion: the outer mapping must apply to the inner generator's
    # MAPPED values, not its base range (Issue #9200 S2 collect-fusion regression).
    @test collect(x + 100 for x in (y * y for y in 1:3)) == [101, 104, 109]
    g = (y * y for y in 1:3)
    @test collect(x + 100 for x in g) == [101, 104, 109]
    @test sum(x + 100 for x in (y * y for y in 1:3)) == 314
    @test first(x + 100 for x in (y * y for y in 1:3)) == 101
    for_out = Int[]
    for v in (x + 100 for x in (y * y for y in 1:3))
        push!(for_out, v)
    end
    @test for_out == [101, 104, 109]
end

@testset "laziness: side effects fire at iteration, not construction" begin
    log = Int[]
    # An inline (non-`f(var)`) body is lifted; the lifted function captures the
    # block-local `log` and the push! fires only at iteration.
    g = (begin
        push!(log, x)
        x * x
    end for x in 1:3)
    @test isempty(log)                # construction ran nothing
    @test sum(g) == 14
    @test log == [1, 2, 3]           # side effects, in iteration order
end

@testset "iterator expression is evaluated once, at construction" begin
    calls = Ref(0)
    g = (x for x in (calls[] += 1; 1:3))
    @test calls[] == 1                # eager: the source was built at construction
    @test collect(g) == [1, 2, 3]
    @test calls[] == 1                # not re-evaluated per element
end

@testset "an error in the body fires at iteration, not construction" begin
    g = (x == 2 ? error("boom") : x for x in 1:3)   # constructing must not throw
    @test_throws ErrorException collect(g)
end

@testset "collect eltype is preserved" begin
    @test eltype(collect(x * x for x in 1:5)) == Int64
    @test eltype(collect(Float64(x) for x in 1:5)) == Float64
end

@testset "IteratorSize / length / size unchanged (post S1 / #9379)" begin
    @test Base.IteratorSize(x * x for x in 1:5) isa Base.HasShape{1}
    @test length(x * x for x in 1:5) == 5
    @test size(x * x for x in 1:5) == (5,)
end

true
