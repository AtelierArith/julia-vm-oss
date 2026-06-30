using Test

# Issue #7203: in a matrix/hcat/vcat literal, a row element that is itself an
# array or range (not a scalar) must be flattened/materialized into the result
# the way upstream Julia does, rather than boxed as an `Any` element.

@testset "matrix-literal concatenation flattens array/range elements (#7203)" begin
    g = [1 2 3]

    # hcat: row-matrix + scalar element -> 1x4 Int matrix (not Any[[1 2 3] 4]).
    gh = [g 4]
    @test gh == [1 2 3 4]
    @test typeof(gh) === Matrix{Int64}
    @test size(gh) == (1, 4)
    @test eltype(gh) === Int64

    # hcat: range elements are materialized column-wise.
    r = [1:2 3:4]
    @test r == [1 3; 2 4]
    @test typeof(r) === Matrix{Int64}
    @test size(r) == (2, 2)

    # hcat: space-separated bracketed matrices ([[1 2] [3 4]]) concatenate
    # horizontally instead of raising a BoundsError.
    mm = [[1 2] [3 4]]
    @test mm == [1 2 3 4]
    @test typeof(mm) === Matrix{Int64}
    @test size(mm) == (1, 4)

    # hcat: three bracketed matrices.
    mmm = [[1 2] [3 4] [5 6]]
    @test mmm == [1 2 3 4 5 6]
    @test size(mmm) == (1, 6)

    # vcat: stacking a row-matrix on top of another row.
    v = [g; [4 5 6]]
    @test v == [1 2 3; 4 5 6]
    @test typeof(v) === Matrix{Int64}
    @test size(v) == (2, 3)

    # vcat: range + scalar flattens to a 1-D Vector (not an N x 1 matrix).
    vv = [1:2; 3]
    @test vv == [1, 2, 3]
    @test typeof(vv) === Vector{Int64}
    @test size(vv) == (3,)

    # hvcat: 2x2 grid of 2x2 matrix blocks.
    a = [1 2; 3 4]
    b = [5 6; 7 8]
    c = [9 10; 11 12]
    d = [13 14; 15 16]
    block = [a b; c d]
    @test block == [1 2 5 6; 3 4 7 8; 9 10 13 14; 11 12 15 16]
    @test typeof(block) === Matrix{Int64}
    @test size(block) == (4, 4)

    # Mixed eltype hcat promotes like upstream.
    pm = [1.0 2; 3 4]
    @test pm == [1.0 2.0; 3.0 4.0]
    @test typeof(pm) === Matrix{Float64}

    # Plain scalar matrix literals are unaffected (fast path preserved).
    s = [1 2; 3 4]
    @test s == [1 2; 3 4]
    @test typeof(s) === Matrix{Int64}
    @test size(s) == (2, 2)

    # Scalars before an array element are flattened too.
    pre = [4 g]
    @test pre == [4 1 2 3]
    @test size(pre) == (1, 4)
end

true
