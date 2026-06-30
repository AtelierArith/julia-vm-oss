# Issue #3549: heterogeneous array literals containing `nothing` (or `missing`)
# plus exactly one other concrete type should report the parametric Union
# element type from `typeof`/`eltype`, not collapse to `Any`.
using Test

@testset "Issue #3549 heterogeneous array Union element types" begin
    a = [1, nothing, 2]
    @test typeof(a) === Vector{Union{Nothing, Int64}}
    @test eltype(a) === Union{Nothing, Int64}

    b = [1.5, nothing, 2.5]
    @test typeof(b) === Vector{Union{Nothing, Float64}}
    @test eltype(b) === Union{Nothing, Float64}

    c = ["x", nothing, "y"]
    @test typeof(c) === Vector{Union{Nothing, String}}
    @test eltype(c) === Union{Nothing, String}

    d = [1, missing, 2]
    @test typeof(d) === Vector{Union{Missing, Int64}}
    @test eltype(d) === Union{Missing, Int64}

    # Homogeneous cases must remain unchanged.
    @test typeof([1, 2, 3]) === Vector{Int64}
    @test typeof([1.0, 2.0]) === Vector{Float64}
end

true
