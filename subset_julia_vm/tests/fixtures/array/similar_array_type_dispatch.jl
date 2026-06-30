using Test

@testset "similar Array type dispatch" begin
    v = similar(Array{Int64}, (3,))
    @test size(v) == (3,)
    @test length(v) == 3
    v[1] = 10
    v[2] = 20
    v[3] = 30
    @test v[2] == 20

    w = similar(Array{Int64}, 2)
    @test size(w) == (2,)
    w[1] = 7
    @test w[1] == 7

    m = similar(Array{Float64}, 2, 2)
    @test size(m) == (2, 2)
    m[2, 2] = 1
    @test m[2, 2] == 1.0
end

true
