# HOF nested inside another HOF's lambda (Issue #5229)
#
# A higher-order call whose lambda argument itself contains another
# higher-order call (e.g. `map(x -> map(j -> j, 1:x), v)`) used to leak a raw
# `StructRef` / wrong value, because the inner HOF clobbered the single-slot
# broadcast (and lazy-generator-iterate) state of the outer HOF. The states are
# now per-call stacks, so nested HOFs no longer overwrite each other.
#
# Verified against upstream Julia 1.12.

using Test

double(x) = x * 2

@testset "HOF nested in lambda (Issue #5229)" begin
    v = [1, 2, 3]

    # map inside a map's lambda — triangular nested arrays
    @test map(x -> map(j -> j, 1:x), v) == [[1], [1, 2], [1, 2, 3]]

    # inner map applies a function, captures the outer iteration variable
    @test map(x -> map(j -> j * x, 1:x), v) == [[1], [2, 4], [3, 6, 9]]

    # named inner function
    @test map(x -> map(double, 1:x), v) == [[2], [2, 4], [2, 4, 6]]

    # filter inside a map's lambda
    @test map(x -> filter(j -> j > 1, 1:x), v) == [Int64[], [2], [2, 3]]

    # comprehension inside a map's lambda (already worked; regression guard)
    @test map(x -> [j for j in 1:x], v) == [[1], [1, 2], [1, 2, 3]]

    # outer over a range, inner over a vector
    @test map(x -> map(j -> j, [10, 20]), 1:3) == [[10, 20], [10, 20], [10, 20]]

    # nested inside a function body with a captured constant
    f(w, k) = map(x -> map(j -> j, 1:k), w)
    @test f([10, 20], 2) == [[1, 2], [1, 2]]

    # three levels of nesting
    @test map(x -> map(y -> map(z -> z, 1:y), 1:x), [1, 2]) ==
          [[[1]], [[1], [1, 2]]]

    # sum reduction inside a map's lambda
    @test map(x -> sum(1:x), v) == [1, 3, 6]

    # ordinary (non-nested) HOFs still behave (regression guard)
    @test map(double, v) == [2, 4, 6]
    @test filter(j -> j > 1, v) == [2, 3]
    @test map(x -> filter(j -> j > 1, [j for j in 1:x]), v) == [Int64[], [2], [2, 3]]
end

true
