using Test

@testset "Float32 colon range length uses Float32 semantics" begin
    r = 0.1f0:0.1f0:0.5f0
    @test length(r) == 5
    @test collect(r) == Float32[0.1f0, 0.2f0, 0.3f0, 0.4f0, 0.5f0]
    @test typeof(collect(r)) == Vector{Float32}
    @test r[5] == 0.5f0
    @test last(r) == 0.5f0

    descending = 0.5f0:-0.1f0:0.1f0
    @test length(descending) == 5
    @test collect(descending) == Float32[0.5f0, 0.4f0, 0.3f0, 0.2f0, 0.1f0]
    @test typeof(collect(descending)) == Vector{Float32}
    @test descending[5] == 0.1f0
    @test last(descending) == 0.1f0
end

true
