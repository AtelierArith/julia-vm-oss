# Base.promote_typejoin (Union-aware typejoin) — Issue #5113
#
# promote_typejoin computes a type containing both arguments: it falls back to
# typejoin, but PRESERVES Nothing/Missing as a small Union rather than widening.
# Verified against upstream Julia 1.12:
#   Base.promote_typejoin(Int, Float64) === Real
#   Base.promote_typejoin(Int, Nothing) === Union{Nothing,Int64}
#   Base.promote_typejoin(Int, Missing) === Union{Missing,Int64}

using Test

@testset "promote_typejoin (Issue #5113)" begin
    # Non-Union path: falls back to typejoin
    @test Base.promote_typejoin(Int, Float64) === Real
    @test Base.promote_typejoin(Int, Int) === Int64
    @test Base.promote_typejoin(Float64, Float64) === Float64
    @test Base.promote_typejoin(Int, String) === Any

    # Nothing / Missing are kept as a small Union (not widened to Any/Real)
    @test Base.promote_typejoin(Int, Nothing) === Union{Nothing,Int64}
    @test Base.promote_typejoin(Nothing, Int) === Union{Nothing,Int64}
    @test Base.promote_typejoin(Int, Missing) === Union{Missing,Int64}

    # promote_type still widens (contrast: promote_typejoin keeps the Union)
    @test promote_type(Int, Float64) === Float64
end

true
