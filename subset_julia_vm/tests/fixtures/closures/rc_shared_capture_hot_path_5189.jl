# Issue #5189: closure captures are stored behind an `Rc`, so cloning a closure
# at every call site (once per HOF iteration) is an O(1) refcount bump that
# shares the frozen capture set instead of deep-cloning the whole capture Vec.
# This fixture locks in that the behavior of capture-heavy HOF workloads — the
# `map(x -> a*x + b, big_arr)` pattern the issue calls out — is unchanged.

using Test

# Closure capturing TWO outer variables, applied per-element over a large array.
function closures_affine_map_5189(a, b, xs)
    map(x -> a * x + b, xs)
end

# A single named closure REUSED across multiple HOF calls. Each call clones the
# closure value (Rc bump) rather than the capture storage.
function closures_reused_closure_5189(a)
    f(x) = x + a
    m1 = map(f, [1, 2, 3])
    m2 = map(f, [10, 20, 30])
    m3 = map(f, [100, 200, 300])
    (m1, m2, m3)
end

# Returned closure called many times (repeated capture borrow on the hot path).
function closures_make_adder_5189(a)
    x -> x + a
end

function closures_apply_returned_many_5189(a, xs)
    adder = closures_make_adder_5189(a)
    map(adder, xs)
end

# Closure feeding a filtered comprehension (capture used N times in the body).
function closures_filtered_capture_5189(a, xs)
    [a * x for x in xs if x > 2]
end

@testset "Rc-shared closure capture hot path (Issue #5189)" begin
    big = collect(1:1000)
    mapped = closures_affine_map_5189(2, 3, big)
    @test mapped == [2 * x + 3 for x in big]
    @test typeof(mapped) == Vector{Int64}
    @test length(mapped) == 1000
    @test mapped[1] == 5
    @test mapped[end] == 2003

    reused = closures_reused_closure_5189(100)
    @test reused[1] == [101, 102, 103]
    @test reused[2] == [110, 120, 130]
    @test reused[3] == [200, 300, 400]

    applied = closures_apply_returned_many_5189(7, [1, 2, 3, 4])
    @test applied == [8, 9, 10, 11]
    @test typeof(applied) == Vector{Int64}

    filtered = closures_filtered_capture_5189(10, [1, 2, 3, 4, 5])
    @test filtered == [30, 40, 50]

    # Float captures: ensure the capture path is type-agnostic.
    mappedf = closures_affine_map_5189(1.5, 0.5, [2.0, 4.0])
    @test mappedf == [3.5, 6.5]
    @test typeof(mappedf) == Vector{Float64}
end

true
