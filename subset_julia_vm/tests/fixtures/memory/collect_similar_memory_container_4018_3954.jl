using Test

@testset "Memory collect_similar container allocation (Issues #3954/#4018)" begin
    memory_values = Base.collect_similar(Memory{Float64}(undef, 0), (1, 2, 3))
    @test typeof(memory_values) == Memory{Int64}
    @test eltype(memory_values) == Int64
    @test length(memory_values) == 3
    @test memory_values[1] == 1
    @test memory_values[2] == 2
    @test memory_values[3] == 3
    memory_values[2] = 20
    @test memory_values[2] == 20

    matrix_values = Base.collect_similar(Memory{Float64}(undef, 0), [1 2; 3 4])
    @test typeof(matrix_values) == Matrix{Int64}
    @test eltype(matrix_values) == Int64
    @test size(matrix_values) == (2, 2)
    @test matrix_values[1, 1] == 1
    @test matrix_values[2, 1] == 3
    @test matrix_values[1, 2] == 2
    @test matrix_values[2, 2] == 4
    matrix_values[1, 2] = 22
    @test matrix_values[1, 2] == 22
end

true
