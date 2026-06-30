# Regression: iterate / collect on Pure Julia iterator wrappers and ranges.
# Direct calls to `iterate(...)` / `collect(...)` should now go through method
# dispatch first (Issue #3735), reaching the Pure Julia methods in
# base/iterators.jl, base/range.jl, base/generator.jl, base/dict.jl,
# base/subarray.jl. Primitive Array/Tuple/String/Range still hit the BuiltinId
# fallback so legacy code continues to work.

using Test

@testset "collect on Pure Julia iterators (Issue #3735)" begin
    # Iterators.take is a Pure Julia struct (Take) with iterate methods in
    # base/iterators.jl. The generic collect(itr) Pure Julia routine should
    # consume it via iterate.
    @test (collect(Iterators.take(1:10, 3)) == [1, 2, 3])
    @test (collect(Iterators.take(1:4, 2)) == [1, 2])
    @test (collect(Iterators.drop(1:5, 2)) == [3, 4, 5])

    # enumerate -> Vector of tuples (i, x)
    pairs_out = collect(enumerate([10, 20, 30]))
    @test (length(pairs_out) == 3)
    @test (pairs_out[1] == (1, 10))
    @test (pairs_out[2] == (2, 20))
    @test (pairs_out[3] == (3, 30))
end

@testset "collect on Pure Julia ranges and primitives (Issue #3735)" begin
    @test (collect(1:3) == [1, 2, 3])
    @test (collect(1:2:7) == [1, 3, 5, 7])

    # Native Array passes through the BuiltinId fallback.
    @test (collect([10, 20, 30]) == [10, 20, 30])
end

@testset "iterate primitives still work (Issue #3735)" begin
    # iterate(::Array)
    arr = [1, 2, 3]
    s = iterate(arr)
    @test (s !== nothing)
    @test (s[1] == 1)
    s2 = iterate(arr, s[2])
    @test (s2 !== nothing)
    @test (s2[1] == 2)

    # iterate(::Range)
    r = 10:12
    t = iterate(r)
    @test (t !== nothing)
    @test (t[1] == 10)
end

true
