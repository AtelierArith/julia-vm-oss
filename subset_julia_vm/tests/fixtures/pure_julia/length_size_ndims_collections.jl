# Test that length/size/ndims work for the primitive containers and ranges
# whose methods live in Pure Julia (Issue #3736). Direct calls to
# `length(...)` / `size(...)` / `ndims(...)` must reach either a Pure Julia
# method (for ranges, dicts, sets, iterators, subarrays, broadcasts) or the
# BuiltinId fallback (for native Array/Tuple/String/Generator).

using Test

@testset "length on primitive containers (Issue #3736)" begin
    @test (length([1, 2, 3]) == 3)
    @test (length((10, 20, 30, 40)) == 4)
    @test (length("hello") == 5)
end

@testset "length on Pure Julia ranges and dict-like (Issue #3736)" begin
    # OneTo / StepRangeLen / LinRange via base/range.jl Pure Julia length method.
    @test (length(1:5) == 5)
    @test (length(1:2:10) == 5)
    @test (length(LinRange(0.0, 1.0, 11)) == 11)

    d = Dict("a" => 1, "b" => 2, "c" => 3)
    @test (length(d) == 3)

    s = Set([1, 2, 3, 4])
    @test (length(s) == 4)
end

@testset "size / ndims on primitive Array (Issue #3736)" begin
    a = [1 2 3; 4 5 6]
    @test (size(a) == (2, 3))
    @test (size(a, 1) == 2)
    @test (size(a, 2) == 3)
    @test (ndims(a) == 2)

    v = [10, 20, 30]
    @test (size(v) == (3,))
    @test (ndims(v) == 1)
end

true
