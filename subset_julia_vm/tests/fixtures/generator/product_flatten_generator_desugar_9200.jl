# Issue #9200 (S4): the PRODUCT (comma multi-binding) and FLATTEN (whitespace
# nested-`for`) generator forms are desugared in lowering to the upstream
# `julia-syntax.scm` `expand-generator` shapes:
#
#   (f(x,y) for x in a, y in b)       => Base.Generator(func, Iterators.product(a, b))
#   (f(x,y) for x in a for y in b)    => Iterators.flatten(Generator(x -> Generator(y -> f(x,y), b), a))
#
# `func` maps the product's destructured tuple. A comma product carries the
# product's N-D shape, so `collect` yields a Matrix (matching `[f for x, y]`); a
# flatten yields a Vector. A filter wraps the innermost iterator in an
# `Iterators.Filter` per nesting level. Every assertion is verified against
# upstream julia 1.12.

using Test

@testset "product generator collects to a Matrix (shape parity)" begin
    m = collect(x + y for x in 1:2, y in 1:3)
    @test m == [2 3 4; 3 4 5]
    @test m isa Matrix{Int64}
    @test size(m) == (2, 3)

    m2 = collect(x * y for x in 1:3, y in 1:4)
    @test m2 == [1 2 3 4; 2 4 6 8; 3 6 9 12]
    @test size(m2) == (3, 4)

    # Array (not range) components keep their shape too.
    A = [10, 20]
    B = [1, 2, 3]
    @test collect(a + b for a in A, b in B) == [11 12 13; 21 22 23]

    # A tuple-valued body over a product (indexed elementwise — a tuple-element
    # matrix literal is a separate unsupported construct in sjulia, Issue #9437).
    mt = collect((x, y) for x in 1:2, y in 10:11)
    @test size(mt) == (2, 2)
    @test mt[1, 1] == (1, 10)
    @test mt[2, 1] == (2, 10)
    @test mt[1, 2] == (1, 11)
    @test mt[2, 2] == (2, 11)
end

@testset "product generator: 3-D product shape" begin
    c = collect(x + y + z for x in 1:2, y in 1:2, z in 1:2)
    @test size(c) == (2, 2, 2)
    @test c[1, 1, 1] == 3
    @test c[2, 2, 2] == 6
    @test c[2, 1, 2] == 5
end

@testset "product generator: consumers via the iterate protocol" begin
    @test sum(x * y for x in 1:3, y in 1:3) == 36
    @test first(x + y for x in 1:2, y in 1:3) == 2
    @test prod(x for x in 1:2, y in 1:3) == 8
    @test eltype(collect(x + y for x in 1:2, y in 1:3)) == Int64
end

@testset "product generator with a filter collapses to a Vector" begin
    # A filtered product is SizeUnknown (like upstream Iterators.Filter), so the
    # result is a Vector, not a Matrix.
    v = collect(x + y for x in 1:2, y in 1:3 if x < y)
    @test v == [3, 4, 5]
    @test v isa Vector{Int64}
    @test sum(x * y for x in 1:3, y in 1:3 if x != y) == 22
end

@testset "product generator: laziness (side effects fire at iteration)" begin
    log = Int[]
    g = (begin
        push!(log, x)
        x * 10
    end for x in 1:2, y in 1:2)
    @test isempty(log)                   # construction ran nothing
    m = collect(g)
    @test size(m) == (2, 2)
    @test log == [1, 2, 1, 2]            # column-major iteration order
end

@testset "flatten generator collects to a Vector (values + order)" begin
    # NB: only the VALUES/order are asserted, not the element type — a flatten of
    # generators collects to Vector{Any} in sjulia vs Vector{Int64} upstream
    # (Issue #9438, deferred; the values are always correct).
    @test collect(x + y for x in 1:2 for y in 1:3) == [2, 3, 4, 3, 4, 5]
    @test collect(x * 10 + y for x in 1:2 for y in 1:2) == [11, 12, 21, 22]
    # A dependent inner range (b depends on the outer x).
    @test collect(y for x in 1:3 for y in 1:x) == [1, 1, 2, 1, 2, 3]
end

@testset "flatten generator: consumers via the iterate protocol" begin
    @test sum(x + y for x in 1:2 for y in 1:3) == 21
    @test first(x + y for x in 1:2 for y in 1:3) == 2

    total = 0
    for v in (x + y for x in 1:2 for y in 1:3)
        total += v
    end
    @test total == 21
end

@testset "flatten generator with a filter (Issue #9325)" begin
    # A flatten whose inner iteration carries an `if` — the combination that was
    # rejected as "nested generators not supported" on main.
    @test sort(collect(x + y for x in 1:3 for y in 1:3 if x < y)) == [3, 4, 5]
    @test sum(x * y for x in 1:3 for y in 1:3 if x != y) == 22
    @test collect(x + y for x in 1:3 for y in 1:3 if x < y) == [3, 4, 5]
end

@testset "flatten generator: laziness (side effects fire at iteration)" begin
    log = Int[]
    g = (begin
        push!(log, x * 10 + y)
        x + y
    end for x in 1:2 for y in 1:2)
    @test isempty(log)
    @test collect(g) == [2, 3, 3, 4]
    @test log == [11, 12, 21, 22]
end

true
