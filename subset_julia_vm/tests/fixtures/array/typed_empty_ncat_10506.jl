using Test

@testset "typed empty ncat literals (Issue #10506)" begin
    v = Int64[;]
    @test v isa Vector{Int64}
    @test size(v) == (0,)

    m = Float64[;;]
    @test m isa Matrix{Float64}
    @test size(m) == (0, 0)

    a3 = Vector{Int64}[;;;]
    @test a3 isa Array{Vector{Int64},3}
    @test size(a3) == (0, 0, 0)
end

true
