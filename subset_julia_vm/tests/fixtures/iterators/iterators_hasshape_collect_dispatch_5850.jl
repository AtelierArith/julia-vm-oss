# Issue #5850: `Base._collect(cont, itr, IteratorEltype(itr), IteratorSize(itr))`
# must dispatch to the more-specific `_collect(..., ::HasShape)` method, not the
# `::IteratorSize` catch-all. The compiler statically resolves this call on the
# inferred type of `IteratorSize(itr)`; that was widened to the abstract
# `IteratorSize` (so only the catch-all matched). `IteratorSize` over a
# statically-ranked array now infers the concrete `HasShape{N}` (mirroring
# upstream `IteratorSize(::AbstractArray{T,N}) = HasShape{N}()`), so the
# shape-preserving method is selected and `collect` keeps the array's shape.

using Test

@testset "IteratorSize concrete HasShape{N} + shape-preserving collect (Issue #5850)" begin
    m = [1 2; 3 4]
    v = [10, 20, 30]

    # IteratorSize over a ranked array is the concrete HasShape{N}
    @test Base.IteratorSize(m) isa Base.HasShape{2}
    @test Base.IteratorSize(v) isa Base.HasShape{1}

    # collect preserves the shape (2-D stays a Matrix, 1-D stays a Vector)
    @test collect(m) == m
    @test size(collect(m)) == (2, 2)
    @test collect(m) isa Matrix{Int64}
    @test collect(v) == v
    @test collect(v) isa Vector{Int64}

    # the explicit _collect form (the issue's reproduction) keeps the 2-D shape
    r = Base._collect([0 0; 0 0], m, Base.IteratorEltype(m), Base.IteratorSize(m))
    @test size(r) == (2, 2)
    @test r == m

    # a Float matrix and a 1xN row matrix also round-trip
    fm = [1.0 2.0; 3.0 4.0]
    @test collect(fm) == fm
    @test size(collect(fm)) == (2, 2)
    @test collect([7 8 9]) == [7 8 9]

    # generators (HasLength / SizeUnknown) are unaffected — flatten to a Vector
    @test collect(x^2 for x in 1:3) == [1, 4, 9]
end

true
