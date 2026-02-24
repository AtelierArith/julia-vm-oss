# Test that Array creation functions use Memory internally (Issue #2762)
# These tests verify zeros/ones/similar produce correct results after
# the builtin migration from direct ArrayValue to Memory-based allocation.

using Test

@testset "Array creation via Memory pipeline" begin
    # zeros - F64 default
    z1 = zeros(3)
    @test length(z1) == 3
    @test z1[1] == 0.0
    @test z1[2] == 0.0
    @test z1[3] == 0.0
    @test eltype(z1) == Float64

    # zeros - 2D
    z2 = zeros(2, 3)
    @test size(z2) == (2, 3)
    @test z2[1, 1] == 0.0
    @test z2[2, 3] == 0.0

    # zeros - typed Int64
    zi = zeros(Int64, 4)
    @test length(zi) == 4
    @test zi[1] == 0
    @test eltype(zi) == Int64

    # ones - F64 default
    o1 = ones(3)
    @test length(o1) == 3
    @test o1[1] == 1.0
    @test o1[2] == 1.0
    @test o1[3] == 1.0
    @test eltype(o1) == Float64

    # ones - 2D
    o2 = ones(2, 3)
    @test size(o2) == (2, 3)
    @test o2[1, 1] == 1.0
    @test o2[2, 3] == 1.0

    # ones - typed Int64
    oi = ones(Int64, 4)
    @test length(oi) == 4
    @test oi[1] == 1
    @test eltype(oi) == Int64

    # similar - same shape
    a = [1.0, 2.0, 3.0]
    s = similar(a)
    @test length(s) == 3
    @test eltype(s) == Float64

    # similar - new length
    s2 = similar(a, 5)
    @test length(s2) == 5
    @test eltype(s2) == Float64

    # Array{T}(undef, n) - Float64
    uf = Array{Float64}(undef, 3)
    @test length(uf) == 3
    @test eltype(uf) == Float64

    # Array{T}(undef, n) - Int64
    ui = Array{Int64}(undef, 4)
    @test length(ui) == 4
    @test eltype(ui) == Int64

    # Array{T}(undef, n) - Bool
    ub = Array{Bool}(undef, 2)
    @test length(ub) == 2
    @test eltype(ub) == Bool

    # Mutability after creation
    z = zeros(3)
    z[1] = 42.0
    @test z[1] == 42.0
    @test z[2] == 0.0

    o = ones(3)
    push!(o, 4.0)
    @test length(o) == 4
    @test o[4] == 4.0
end

true
