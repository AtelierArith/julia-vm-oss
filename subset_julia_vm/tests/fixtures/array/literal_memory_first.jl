# Array literal behavior covered while VM builder storage is Memory-first.

using Test

@testset "Array literal Memory-first builder behavior" begin
    v = [1, 2, 3]
    @test typeof(v) == Vector{Int64}
    @test eltype(v) == Int64
    @test size(v) == (3,)
    @test v[2] == 2

    v[2] = 20
    @test v[2] == 20
    @test typeof(v) == Vector{Int64}

    m = [1.0 2.0; 3.0 4.0]
    @test typeof(m) == Array{Float64, 2}
    @test eltype(m) == Float64
    @test size(m) == (2, 2)
    @test m[1, 2] == 2.0

    m[2, 1] = 30.0
    @test m[2, 1] == 30.0
    @test m[3] == 2.0

    b = [true, false, true]
    @test typeof(b) == Vector{Bool}
    @test eltype(b) == Bool
    b[2] = true
    @test b[1] && b[2] && b[3]
end

true
