using Test

struct SplatLiteralPoint7255
    x::Int
    y::Int
end

# Array-literal positional splat (Issue #7255).
# Both untyped `[a, xs..., b]` and typed `T[a, xs..., b]` literals must spread
# the splatted iterable's elements into the array, matching upstream Julia's
# `Base.vect` / `getindex(::Type{T}, vals...)` lowering.
@testset "array splat literal (#7255)" begin
    # untyped only-splat
    g1(pts...) = [pts...]
    @test g1(1, 2, 3) == [1, 2, 3]
    @test eltype(g1(1, 2, 3)) == Int64
    @test g1() == []
    @test eltype(g1()) == Any

    # untyped mixed: scalar, splat, scalar
    g2(pts...) = [0, pts..., 99]
    @test g2(1, 2, 3) == [0, 1, 2, 3, 99]
    @test eltype(g2(1, 2, 3)) == Int64
    @test g2() == [0, 99]

    # untyped promotion across the splat
    g3(pts...) = [pts...]
    @test g3(1, 2.0) == [1.0, 2.0]
    @test eltype(g3(1, 2.0)) == Float64

    # typed Any[...]
    ga(pts...) = Any[pts...]
    @test ga(1, 2, 3) == [1, 2, 3]
    @test eltype(ga(1, 2, 3)) == Any
    @test ga() == []
    @test eltype(ga()) == Any

    # typed Float64[...]
    gf(pts...) = Float64[pts...]
    @test gf(1, 2, 3) == [1.0, 2.0, 3.0]
    @test eltype(gf(1, 2, 3)) == Float64

    # typed mixed Int[...]
    gi(pts...) = Int[0, pts..., 99]
    @test gi(1, 2, 3) == [0, 1, 2, 3, 99]
    @test eltype(gi(1, 2, 3)) == Int64

    # splat of an array (not just a tuple) inside a literal
    gv(v) = [10, v..., 20]
    @test gv([1, 2, 3]) == [10, 1, 2, 3, 20]

    # splat of a range
    gr() = [0, (1:3)..., 9]
    @test gr() == [0, 1, 2, 3, 9]

    # multiple splats
    gm(a, b) = [a..., b...]
    @test gm((1, 2), (3, 4)) == [1, 2, 3, 4]

    # parametric type target (`Complex{Float64}[xs...]`)
    gc(pts...) = Complex{Float64}[pts...]
    @test gc(1, 2, 3) == Complex{Float64}[1, 2, 3]
    @test eltype(gc(1, 2, 3)) == Complex{Float64}

    # user-struct element type target. The element values are spread correctly;
    # the array's declared eltype for a user-struct typed literal is a separate,
    # pre-existing limitation shared by the non-splat form `T[...]` (it widens to
    # `Any`), so this fixture only pins the spread values, matching how the
    # non-splat typed-struct literal already behaves in sjulia.
    gp(pts...) = SplatLiteralPoint7255[pts...]
    pts_val = gp(SplatLiteralPoint7255(1, 2), SplatLiteralPoint7255(3, 4))
    @test pts_val == SplatLiteralPoint7255[SplatLiteralPoint7255(1, 2), SplatLiteralPoint7255(3, 4)]
    @test length(pts_val) == 2
    @test pts_val[1] == SplatLiteralPoint7255(1, 2)
end

true
