using Test

# Issue #5744: deleteat!(arr, inds) with a vector/range of indices failed for a
# `copy`/`collect` result (a Memory-backed Array-wrapper StructRef) — only the
# native-array literal/var case worked. The compiled `ArrayDeleteAtIndices` fast
# path now falls back, for a StructRef target, to the pure-Julia
# `deleteat!(a::Array, inds)` (untyped `inds` so the #4189 native-array matcher
# guard does not block selection), mirroring the scalar fallback (#5721).

@testset "deleteat! multi-index on copy/collect result (Issue #5744)" begin
    a = copy([1, 2, 3, 4, 5])
    deleteat!(a, [2, 4])
    @test a == [1, 3, 5]

    b = collect(1:5)
    deleteat!(b, 2:3)
    @test b == [1, 4, 5]

    c = copy([10, 20, 30, 40, 50])
    deleteat!(c, 2:4)
    @test c == [10, 50]

    # deleteat! returns the array
    f = copy([1, 2, 3])
    @test deleteat!(f, [1, 3]) == [2]

    # Controls: literal/var multi-index and scalar still work
    @test deleteat!([1, 2, 3, 4, 5], [2, 4]) == [1, 3, 5]
    g = [1, 2, 3]
    deleteat!(g, 2)
    @test g == [1, 3]
    h = collect(1:4)
    deleteat!(h, [1, 2])
    @test h == [3, 4]
end

true
