# Issue #9379 (trait-layer symptom of #9320; foundation slice of the #9200
# generator-desugar epic): `Base.IteratorSize` and `size` of a generator must
# match upstream. Upstream models `(x for x in it if p(x))` as
# `Generator(map, Iterators.Filter(p, it))`, and `IteratorSize(::Type{<:Filter})
# == SizeUnknown()`, so a FILTERED generator's size is unknown and `size`
# reaches the undefined `size(::Filter)` -> MethodError. sjulia collapses the
# filter into the native generator `callable` (the base iterator stays in
# `g.iter`), so these traits are resolved via the SAME structural
# `callable`-variant check as `length`/`isempty` (Issue #9320) — driven off the
# representation, NOT a type-name string.
#
# Every assertion below is at PARITY with upstream julia 1.12 (verified with
# `julia --startup-file=no`). The one KNOWN residual divergence — the
# TYPE-LEVEL `Base.IteratorSize(typeof(filtered_gen))` (upstream `SizeUnknown()`,
# sjulia `HasShape{1}()`) — is deliberately NOT asserted here: sjulia's `typeof`
# reports the *base* iterator (`Base.Generator{UnitRange{Int64}, ...}`), not the
# conceptual `Generator{Filter{...}, ...}`, so the type alone cannot see the
# filter. That is resolved once a real `Iterators.Filter` lands in `g.iter` via
# the Filter-desugar slice (#9200 S3), and is left out of this parity fixture.

using Test

@testset "IteratorSize(generator) value-level parity (Issue #9379)" begin
    gf = (x for x in 1:5 if x > 2)   # filtered
    gu = (x for x in 1:5)            # unfiltered

    @test gf isa Base.Generator
    @test gu isa Base.Generator

    # filtered -> SizeUnknown (the #9379 fix); unfiltered -> HasShape{1}
    @test Base.IteratorSize(gf) isa Base.SizeUnknown
    @test Base.IteratorSize(gu) isa Base.HasShape{1}

    # captured-predicate (runtime-callable) filtered path also SizeUnknown
    k = 2
    @test Base.IteratorSize((x for x in 1:5 if x > k)) isa Base.SizeUnknown

    # array-base + named-predicate filtered path
    over3(x) = x > 3
    @test Base.IteratorSize((x for x in [1, 2, 3, 4, 5] if over3(x))) isa Base.SizeUnknown

    # array-wrapper bases keep upstream shape rank at the value level (#9393)
    @test Base.IteratorSize((2x for x in [10, 20, 30])) isa Base.HasShape{1}
    @test Base.IteratorSize((2x for x in [1 2; 3 4])) isa Base.HasShape{2}
    @test Base.IteratorSize((x for x in reshape(collect(1:8), 2, 2, 2))) isa Base.HasShape{3}
end

@testset "IteratorSize/IteratorEltype(typeof(unfiltered)) parity (Issue #9379)" begin
    gu = (x for x in 1:5)
    @test Base.IteratorSize(typeof(gu)) isa Base.HasShape{1}
    @test Base.IteratorEltype(gu) isa Base.EltypeUnknown
    @test Base.IteratorEltype(typeof(gu)) isa Base.EltypeUnknown
end

@testset "size(generator) parity (Issue #9379)" begin
    gu = (x for x in 1:5)
    gf = (x for x in 1:5 if x > 2)

    # unfiltered delegates to the base iterator: size(1:5) == (5,)
    @test size(gu) == (5,)
    @test size((2x for x in [10, 20, 30])) == (3,)
    @test size((2x for x in [1 2; 3 4])) == (2, 2)
    @test size((x for x in reshape(collect(1:8), 2, 2, 2))) == (2, 2, 2)

    # filtered -> MethodError (base is a conceptual Iterators.Filter with no size)
    @test_throws MethodError size(gf)
    k = 2
    @test_throws MethodError size((x for x in 1:5 if x > k))

    # 2-arg size(::Generator, dim) has no method upstream for EITHER shape
    @test_throws MethodError size(gu, 1)
    @test_throws MethodError size(gf, 1)
end

@testset "length(generator) stays consistent with #9320" begin
    @test length((x for x in 1:5)) == 5
    @test_throws MethodError length((x for x in 1:5 if x > 2))
end

@testset "consumers over filtered/unfiltered generators untouched" begin
    @test collect((x for x in 1:5)) == [1, 2, 3, 4, 5]
    @test collect((x for x in 1:5 if x > 2)) == [3, 4, 5]
    @test sum((x for x in 1:5 if x > 2)) == 12
end

true
