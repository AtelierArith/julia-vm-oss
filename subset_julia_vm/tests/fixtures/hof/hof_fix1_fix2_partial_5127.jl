using Test

# Issue #5127: Base.Fix1 / Base.Fix2 partial-application types.
#
# Fix1(f, x)(y) == f(x, y)  (fix the first argument)
# Fix2(f, x)(y) == f(y, x)  (fix the second argument)
#
# These are the concrete partial-application types the standard library uses
# (upstream aliases them to Fix{1}/Fix{2}). Upstream marks Fix1/Fix2 as public
# but does not export them, so they are referenced as Base.Fix1 / Base.Fix2.
# Verified against upstream Julia 1.12.

@testset "Fix1 fixes the first argument" begin
    @test Base.Fix1(-, 10)(3) == 7
    @test Base.Fix1(+, 5)(10) == 15
    @test Base.Fix1(*, 3)(4) == 12
end

@testset "Fix2 fixes the second argument" begin
    @test Base.Fix2(-, 10)(3) == -7
    @test Base.Fix2(^, 2)(3) == 9
    @test Base.Fix2(/, 2)(10) == 5.0
end

@testset "Fix2 with map" begin
    @test map(Base.Fix2(^, 2), [1, 2, 3]) == [1, 4, 9]
    @test map(Base.Fix2(+, 10), [1, 2, 3]) == [11, 12, 13]
end

@testset "Fix1 with map" begin
    @test map(Base.Fix1(-, 10), [1, 2, 3]) == [9, 8, 7]
end

@testset "Fix2 comparison closures with filter / findall" begin
    # `==(x)` / `>(x)` produce equivalent y -> y == x / y -> y > x callables.
    @test findall(==(2), [1, 2, 2, 3]) == [2, 3]
    @test filter(>(2), [1, 2, 3, 4]) == [3, 4]
    @test map(==(1), [1, 2, 1]) == Bool[1, 0, 1]
    @test filter(>(0), [-1, 2, -3]) == [2]
end

@testset "Fix2 membership predicate" begin
    isin = Base.Fix2(in, [1, 2, 3])
    @test isin(2) == true
    @test isin(5) == false
    @test map(Base.Fix2(in, [1, 2, 3]), [0, 1, 4]) == Bool[0, 1, 0]
end

@testset "Fix types are concrete and store fields" begin
    g = Base.Fix2(==, 2)
    @test g.f == (==)
    @test g.x == 2
    @test typeof(g) === Base.Fix2{typeof(==),Int64}
end

true
