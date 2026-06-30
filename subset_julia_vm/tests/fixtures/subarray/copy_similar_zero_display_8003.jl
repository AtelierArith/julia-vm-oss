# Issue #8003: copy/similar/zero on a SubArray (view) must return a fresh
# Vector, and a SubArray must display element-wise like a dense array instead
# of dumping its raw internal struct. Before the fix, `similar`/`copy` errored
# ("similar requires an array or memory argument"), `zero` errored, and
# `println(v)` printed `SubArray{...}(...)`.

using Test

@testset "SubArray copy/similar/zero/display (#8003)" begin
    buf = [1.0, 2.0, 3.0, 4.0, 5.0]
    v = view(buf, 1:3)

    # Already-working paths (regression guard).
    @test sum(v) == 6.0
    @test v[2] == 2.0

    # copy: fresh, independent Vector of the viewed elements.
    c = copy(v)
    @test c isa Vector{Float64}
    @test c == [1.0, 2.0, 3.0]
    c[1] = 99.0
    @test buf == [1.0, 2.0, 3.0, 4.0, 5.0]   # parent untouched
    @test v[1] == 1.0

    # similar: uninitialised Vector of the right length/eltype.
    s = similar(v)
    @test s isa Vector{Float64}
    @test length(s) == 3
    @test eltype(similar(v, Int64)) === Int64
    @test length(similar(v, Int64, (5,))) == 5

    # zero: zero-filled Vector with the view's eltype and shape.
    z = zero(v)
    @test z isa Vector{Float64}
    @test z == [0.0, 0.0, 0.0]

    # Element-wise display (not the raw struct).
    @test string(v) == "[1.0, 2.0, 3.0]"
    @test repr(v) == "[1.0, 2.0, 3.0]"

    # Integer view: copy/zero/display preserve Int64 eltype.
    iv = view([10, 20, 30, 40], 2:3)
    @test copy(iv) == [20, 30]
    @test copy(iv) isa Vector{Int64}
    @test zero(iv) == [0, 0]
    @test string(iv) == "[20, 30]"

    # 2-D range view: copy materialises a Matrix; display is element-wise.
    A = reshape([1.0, 2.0, 3.0, 4.0, 5.0, 6.0], 2, 3)
    mv = view(A, 1:2, 2:3)
    @test copy(mv) == [3.0 5.0; 4.0 6.0]
    @test string(mv) == "[3.0 5.0; 4.0 6.0]"
end

true
