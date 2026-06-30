using Test

@testset "Memory similar vararg dims (Issue #4018)" begin
    m = Memory{Int64}(undef, 3)

    same_matrix = similar(m, 2, 2)
    @test typeof(same_matrix) == Matrix{Int64}
    @test eltype(same_matrix) == Int64
    @test size(same_matrix) == (2, 2)
    @test length(same_matrix) == 4

    typed_matrix = similar(m, Float32, 2, 2)
    @test typeof(typed_matrix) == Matrix{Float32}
    @test eltype(typed_matrix) == Float32
    @test size(typed_matrix) == (2, 2)
    @test length(typed_matrix) == 4
    typed_matrix[2, 2] = Float32(4.5)
    @test typed_matrix[2, 2] == Float32(4.5)

    typed_memory = similar(m, Float32, 3)
    @test typeof(typed_memory) == Memory{Float32}
    @test eltype(typed_memory) == Float32
    @test size(typed_memory) == (3,)
    @test length(typed_memory) == 3
end

true
