# Regression guard for Issue #6846.
#
# Array literals now finalize their backing `Memory` into the `Array{T,N}`
# wrapper natively (a zero-copy `MemoryRef` view) instead of calling the
# per-literal pure-Julia `wrap(::Type{Array}, mem, dims)`. The native finalize
# must reconstruct every element-type storage layout correctly — interleaved
# `Complex`, array-of-struct (AoS), boxed `Any`, plain primitives, multi-dim —
# which the earlier `ArrayValue` round-trip mishandled for the non-primitive
# layouts (it produced an out-of-bounds wrapper).

using Test

struct Pt
    x::Int
    y::Int
end

@testset "array literal native finalize across element types (Issue #6846)" begin
    # plain primitives
    ai = [1, 2, 3]
    @test length(ai) == 3
    @test ai[2] == 2
    af = [1.0, 2.0]
    @test af[1] == 1.0
    ab = [true, false, true]
    @test ab[3] == true
    astr = ["a", "b"]
    @test astr[2] == "b"
    ac = ['x', 'y']
    @test ac[1] == 'x'

    # mixed -> Vector{Any}
    aa = [1, "two", 3.0]
    @test length(aa) == 3
    @test aa[2] == "two"

    # interleaved Complex (this was the regression: out-of-bounds on index)
    ax = [Complex(1.0, 2.0), Complex(3.0, 4.0)]
    @test ax isa Vector{Complex{Float64}}
    @test length(ax) == 2
    @test real(ax[1]) == 1.0
    @test imag(ax[2]) == 4.0
    axi = [1.0 + 2.0im, 3.0 + 4.0im]
    @test imag(axi[1]) == 2.0

    # array-of-struct (AoS)
    ap = [Pt(1, 2), Pt(3, 4)]
    @test length(ap) == 2
    @test ap[1].x == 1
    @test ap[2].y == 4

    # multi-dim literal (column-major)
    m = [1 2; 3 4]
    @test size(m) == (2, 2)
    @test m[2, 1] == 3
    @test m[1, 2] == 2

    # empty typed literal
    e = Int[]
    @test e isa Vector{Int}
    @test length(e) == 0
end

true
