# Issue #9320: `length` of a lazy FILTERED generator must raise a MethodError,
# not silently return the UNFILTERED base length. Upstream models a filtered
# generator as `Generator(map, Iterators.Filter(pred, iter))`, and
# `IteratorSize(::Type{<:Filter}) == SizeUnknown()`, so `length(::Filter)` is
# undefined and `length(g)` throws a MethodError. sjulia previously delegated a
# filtered generator's `length` to its base iterator and returned `5` for
# `(x for x in 1:5 if x > 2)`.
#
# An UNFILTERED generator keeps a well-defined length (delegating to the base
# iterator), and `collect` / `sum` / `for`-iteration over a filtered generator
# stay correct (they drive the iterate protocol, never `length`).

using Test

@testset "length(filtered generator) raises MethodError (Issue #9320)" begin
    h = (x for x in 1:5 if x > 2)
    @test h isa Base.Generator
    @test_throws MethodError length(h)

    # over an array base, and with a named predicate — same MethodError
    over3(x) = x > 3
    ha = (x for x in [1, 2, 3, 4, 5] if over3(x))
    @test_throws MethodError length(ha)

    # a captured-predicate filtered generator (runtime-callable path) too
    k = 2
    hk = (x for x in 1:5 if x > k)
    @test_throws MethodError length(hk)
end

@testset "length(unfiltered generator) stays correct (Issue #9320)" begin
    g = (x for x in 1:5)
    @test length(g) == 5

    g2 = (2x for x in [10, 20, 30])
    @test length(g2) == 3

    # mapped body does not change the length
    g3 = (x^2 for x in 1:7)
    @test length(g3) == 7
end

@testset "collect/sum/for over filtered generators untouched (Issue #9320)" begin
    h = (x for x in 1:5 if x > 2)
    @test collect(h) == [3, 4, 5]

    h2 = (x for x in 1:5 if x > 2)
    @test sum(h2) == 12

    total = 0
    for x in (y for y in 1:5 if y > 2)
        total += x
    end
    @test total == 12
end

@testset "isempty(generator) is iterate-based, not length-based (Issue #9320)" begin
    # Upstream's generic `isempty(itr)` drives the iterate protocol
    # (`iterate(itr) === nothing`, julia/base/essentials.jl), so a filtered
    # generator reports emptiness by its FILTERED contents and never calls
    # `length` — which is a MethodError for a filtered generator (see above).
    # sjulia previously used the length-based generic `isempty(arr) =
    # length(arr) == 0`, which regressed to a MethodError once #9320 made
    # `length(filtered generator)` throw. Both branches below match upstream.

    # filtered, non-empty (3, 4, 5 pass) -> false; fully filtered out -> true
    @test isempty((x for x in 1:5 if x > 2)) == false
    @test isempty((x for x in 1:5 if x > 100)) == true

    # captured-predicate (runtime-callable) filtered path
    k = 2
    @test isempty((x for x in 1:5 if x > k)) == false
    kk = 100
    @test isempty((x for x in 1:5 if x > kk)) == true

    # array base + named predicate
    over3(x) = x > 3
    @test isempty((x for x in [1, 2, 3, 4, 5] if over3(x))) == false
    @test isempty((x for x in [1, 2, 3] if over3(x))) == true

    # unfiltered control: isempty stays correct for empty/non-empty and a
    # mapped body (the map never changes emptiness)
    @test isempty((x for x in 1:5)) == false
    @test isempty((x for x in 1:0)) == true
    @test isempty((x^2 for x in 1:3)) == false
end

@testset "size(filtered generator) raises MethodError (Issue #9379)" begin
    # Upstream models a filtered generator as `Iterators.Filter(pred, iter)`
    # with `IteratorSize == SizeUnknown()`, so `size(filtered)` is a MethodError.
    # sjulia now mirrors that at the Rust `size` builtin using the same
    # structural `callable`-variant check as `length` (Issue #9379 resolved the
    # trait-layer gap that #9320 had deferred). See
    # `generator/iterator_traits_9379.jl` for the full IteratorSize/size parity.
    @test_throws MethodError size((x for x in 1:5 if x > 2))
end

true
