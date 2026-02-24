using Test

@testset "Memory{T} primitive constructor and indexing" begin
    # Basic construction with type parameter
    m = Memory{Int64}(5)
    @test length(m) == 5

    # Indexing (setindex! and getindex)
    m[1] = 10
    m[2] = 20
    m[3] = 30
    @test m[1] == 10
    @test m[2] == 20
    @test m[3] == 30

    # Float64 Memory
    mf = Memory{Float64}(3)
    mf[1] = 1.5
    mf[2] = 2.5
    mf[3] = 3.5
    @test mf[1] == 1.5
    @test mf[2] == 2.5
    @test mf[3] == 3.5

    # Zero-length Memory
    m0 = Memory{Int64}(0)
    @test length(m0) == 0
end

true
