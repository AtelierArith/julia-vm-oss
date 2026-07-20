# Issue #6806 (PR B): indexing a rank-1 `Array{T}` wrapper through an untyped
# parameter compiles to a raw `IndexLoad` and now reads the element directly from
# the MemoryRef-backed storage instead of dispatching `getindex` per index. This
# is a characterization test pinning value and bounds semantics across that
# optimization — both must stay identical to the previous (dispatch) behavior and
# to upstream Julia.
using Test

f(a, i) = a[i]

@testset "rank-1 wrapper indexing via untyped param (Issue #6806)" begin
    # element values across element types
    @test f([10, 20, 30], 2) == 20
    @test f([1.5, 2.5, 3.5], 3) == 3.5
    @test f(["a", "b", "c"], 1) == "a"
    @test f([true, false, true], 2) == false

    # comprehension- and collect-produced wrappers
    v = [i * i for i in 1:5]
    @test f(v, 4) == 16
    @test f(collect(1:2:9), 3) == 5

    # first/last positions
    w = [10, 20, 30, 40, 50]
    @test f(w, 1) == 10
    @test f(w, 5) == 50

    # bounds errors preserved (type)
    @test_throws BoundsError f([1, 2, 3], 5)
    @test_throws BoundsError f([1, 2, 3], 0)
    @test_throws BoundsError f(Int[], 1)

    # mutation through the wrapper still observed on subsequent reads
    m = [1, 2, 3]
    m[2] = 99
    @test f(m, 2) == 99
end

true
