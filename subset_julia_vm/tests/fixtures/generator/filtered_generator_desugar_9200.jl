# Issue #9200 S3: the FILTERED single-scalar-binding generator
# `(f(x) for x in iter if p(x))` desugars in lowering to the upstream
# `Base.Generator(f, Base.Iterators.Filter(p, iter))` shape (julia-syntax.scm
# `expand-generator`). Every consumer drives the iterate protocol, so filtering
# and laziness match upstream; and because `typeof(g).parameters[1]` is now a
# `Base.Iterators.Filter` (`IteratorSize(::Type{<:Filter}) == SizeUnknown()`), the
# TYPE-LEVEL `IteratorSize(typeof(filtered))` reports `SizeUnknown()` — the S1
# leftover (`generator/iterator_traits_9379.jl` deferred this to S3).
#
# Every assertion is verified at PARITY with upstream julia 1.12.

using Test

@testset "filtered generator: consumers drive the iterate protocol (Issue #9200 S3)" begin
    @test collect(x for x in 1:5 if x > 2) == [3, 4, 5]
    @test collect(x^2 for x in 1:6 if x % 2 == 0) == [4, 16, 36]
    @test sum(x for x in 1:5 if x > 2) == 12
    @test first(x for x in 1:5 if x > 2) == 3
    @test count(x -> x > 3, (x for x in 1:5 if x > 2)) == 2
    @test isempty(x for x in 1:5 if x > 2) == false
    @test isempty(x for x in 1:5 if x > 100) == true

    total = 0
    for x in (y for y in 1:5 if y > 2)
        total += x
    end
    @test total == 12

    # captured-predicate (runtime-callable) path
    k = 2
    @test collect(x * 10 for x in 1:5 if x > k) == [30, 40, 50]
end

@testset "TYPE-LEVEL IteratorSize(typeof(filtered)) == SizeUnknown (Issue #9200 S3)" begin
    # The S1 leftover resolved by the Filter desugar: `typeof(g)`'s iterator
    # parameter is now `Iterators.Filter`, so the type alone reports SizeUnknown.
    gf = (x for x in 1:5 if x > 2)
    @test Base.IteratorSize(typeof(gf)) isa Base.SizeUnknown
    @test Base.IteratorSize(gf) isa Base.SizeUnknown          # value-level too

    # captured / named-predicate / array-base variants
    k = 2
    @test Base.IteratorSize(typeof(x for x in 1:5 if x > k)) isa Base.SizeUnknown
    over3(x) = x > 3
    @test Base.IteratorSize(typeof(x for x in [1, 2, 3, 4, 5] if over3(x))) isa Base.SizeUnknown

    # an UNFILTERED generator keeps its base iterator's shape at the type level
    @test Base.IteratorSize(typeof(x for x in 1:5)) isa Base.HasShape{1}
end

@testset "length / size of a filtered generator raise MethodError (Issue #9320 / #9379)" begin
    @test_throws MethodError length(x for x in 1:5 if x > 2)
    @test_throws MethodError size(x for x in 1:5 if x > 2)
    # unfiltered stays well-defined
    @test length(x for x in 1:5) == 5
    @test size(x for x in 1:5) == (5,)
end

@testset "specialized counted-loop function iterates a filtered generator (Issue #9353)" begin
    # A filtered generator flowing into a function that gets specialized on its
    # concrete `Base.Generator` argument type must iterate via the iterate
    # protocol, not a counted `length` + `itr[idx]` loop (a filtered generator is
    # `SizeUnknown` and not integer-indexable). Upstream returns 12.
    function total(itr)
        s = 0
        for x in itr
            s += x
        end
        return s
    end
    h = (x for x in 1:5 if x > 2)
    @test total(h) == 12
    # unfiltered generator through the same specialized function
    @test total(2x for x in 1:5) == 30
    # captured-predicate filtered generator
    k = 2
    @test total(x for x in 1:5 if x > k) == 12
end

@testset "filtered generator is LAZY: side effects and errors fire at iteration (Issue #9200 S3)" begin
    log = String[]
    g = (begin
        push!(log, "body $x")
        x * 10
    end for x in 1:4 if x % 2 == 0)
    @test isempty(log)                        # construction ran nothing
    @test collect(g) == [20, 40]
    @test log == ["body 2", "body 4"]         # only kept elements, in order

    # a body error fires at iteration/collect, not construction
    ge = (error("boom $x") for x in 1:3 if x > 1)
    @test ge isa Base.Generator               # constructed without raising
    @test_throws ErrorException collect(ge)
end

@testset "empty filtered collect preserves the inferred eltype (Issue #9127)" begin
    a = collect(x^2 for x in 1:4 if x > 100)
    @test isempty(a)
    @test eltype(a) == Int64                  # not Any / Union{}
end

true
