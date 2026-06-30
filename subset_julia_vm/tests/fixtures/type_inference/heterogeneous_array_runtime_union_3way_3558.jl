# Issue #3558: heterogeneous array literals mixing 3+ types where the non-
# Nothing/Missing concretes are numeric should apply numeric promotion across
# the concretes and report the parametric Union element type, not collapse to
# `Any`. The 2-way case is covered by Issue #3549.
using Test

@testset "Issue #3558 heterogeneous 3-way Union with promotion" begin
    a = [1, nothing, 2.5]
    @test typeof(a) === Vector{Union{Nothing, Float64}}
    @test eltype(a) === Union{Nothing, Float64}

    b = [1.0, missing, 2]
    @test typeof(b) === Vector{Union{Missing, Float64}}
    @test eltype(b) === Union{Missing, Float64}

    c = [1, 2.0, missing, nothing]
    @test typeof(c) === Vector{Union{Missing, Nothing, Float64}}
    @test eltype(c) === Union{Missing, Nothing, Float64}

    # Both Missing and Nothing alongside a single Int promote naturally.
    d = [1, nothing, missing, 2]
    @test typeof(d) === Vector{Union{Missing, Nothing, Int64}}
    @test eltype(d) === Union{Missing, Nothing, Int64}
end

true
