# Issue #6806 (PR B): writing into a numeric `Array{T}` wrapper through an untyped
# parameter (`a[i] = v`, compiled to a raw `IndexStore`) writes the element
# directly into the MemoryRef-backed storage instead of dispatching `setindex!`
# per write. Characterization of value, coercion, aliasing, and bounds semantics
# across that fast path; verified against upstream Julia 1.12.
using Test

setat!(a, i, v) = (a[i] = v; a)
getat(a, i) = a[i]

@testset "numeric wrapper setindex! via untyped param (Issue #6806)" begin
    # Int storage, Int value
    a = [10, 20, 30]
    setat!(a, 2, 99)
    @test a == [10, 99, 30]
    @test getat(a, 2) == 99

    # Float storage with Int value -> numeric convert to Float64 (matches setindex!)
    b = [1.0, 2.0, 3.0]
    setat!(b, 1, 7)
    @test b[1] === 7.0
    @test eltype(b) === Float64

    # Float value into Float storage
    setat!(b, 3, 3.5)
    @test b[3] === 3.5

    # aliasing: the wrapper is mutated in place, visible through another binding
    c = [0, 0, 0]
    d = c
    setat!(d, 2, 42)
    @test c[2] == 42

    # comprehension- and collect-built wrappers
    v = [i for i in 1:5]
    setat!(v, 5, 500)
    @test v[5] == 500
    w = collect(1:4)
    setat!(w, 1, -1)
    @test w[1] == -1

    # write to a linear position of a matrix wrapper (column-major)
    m = [10i + j for i in 1:2, j in 1:2]
    m[3] = 777          # linear index 3 == m[1,2]
    @test m[1, 2] == 777

    # bounds errors preserved (type)
    @test_throws BoundsError setat!([1, 2, 3], 5, 0)
    @test_throws BoundsError setat!([1, 2, 3], 0, 0)

    # the write returns the collection (IndexStore leaves it for StoreBack)
    r = setat!([1, 2, 3], 1, 9)
    @test r == [9, 2, 3]
end

true
