# Test broadcast result type inference for common patterns (Issue #3464)

using Test

@testset "type_inference_broadcast_inference: broadcast infers element type correctly" begin
    v = [1, 2, 3]
    w = [4, 5, 6]
    fv = [1.0, 2.0, 3.0]

    # Vector .+ Vector -> Vector{Int64}
    @test typeof(v .+ w) == Vector{Int64}
    # Vector .* scalar -> Vector{Int64}
    @test typeof(v .* 2) == Vector{Int64}
    # Vector{Float64} unary broadcast
    @test typeof(sqrt.(fv)) == Vector{Float64}
    # abs.(vector)
    @test typeof(abs.(v)) == Vector{Int64}
end

true
