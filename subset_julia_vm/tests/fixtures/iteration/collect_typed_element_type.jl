using Test

@testset "collect typed element type" begin
    floats = collect(Float64, 1:2:5)
    @test typeof(floats) === Vector{Float64}
    @test eltype(floats) === Float64
    @test floats == [1.0, 3.0, 5.0]

    ints = collect(Int32, (1, 2, 3))
    @test typeof(ints) === Vector{Int32}
    @test eltype(ints) === Int32
    @test length(ints) == 3
    @test ints[1] == Int32(1)
    @test ints[2] == Int32(2)
    @test ints[3] == Int32(3)

    bools = collect(Bool, (1, 0, 1))
    @test typeof(bools) === Vector{Bool}
    @test eltype(bools) === Bool
    @test length(bools) == 3
    @test bools[1] == true
    @test bools[2] == false
    @test bools[3] == true

    typed_matrix = collect(Float64, [1 2; 3 4])
    @test typeof(typed_matrix) === Matrix{Float64}
    @test eltype(typed_matrix) === Float64
    @test size(typed_matrix) == (2, 2)
    @test typed_matrix[1, 1] == 1.0
    @test typed_matrix[2, 2] == 4.0

    similar_values = Base.collect_similar([0.0], (1, 2.0))
    @test typeof(similar_values) === Vector{Real}
    @test eltype(similar_values) === Real
    @test similar_values[1] == 1
    @test similar_values[2] == 2.0
end

true
