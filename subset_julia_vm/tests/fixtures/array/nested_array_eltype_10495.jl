using Test

@testset "Nested array element types stay concrete (Issue #10495)" begin
    copied = Vector([[1], [2]])
    @test copied == [[1], [2]]
    @test typeof(copied) == Vector{Vector{Int64}}
    @test eltype(copied) == Vector{Int64}
    @test typeof(copied[1]) == Vector{Int64}

    undef_vec = Vector{Vector{Int64}}(undef, 2)
    @test typeof(undef_vec) == Vector{Vector{Int64}}
    @test eltype(undef_vec) == Vector{Int64}
    @test length(undef_vec) == 2

    similar_vec = similar(Vector{Vector{Int64}}, 2)
    @test typeof(similar_vec) == Vector{Vector{Int64}}
    @test eltype(similar_vec) == Vector{Int64}

    memory = Memory{Vector{Int64}}(undef, 2)
    @test typeof(memory) == Memory{Vector{Int64}}
    @test eltype(memory) == Vector{Int64}

    mapped = map(collect, [(1, 2), (3, 4)])
    @test mapped == [[1, 2], [3, 4]]
    @test typeof(mapped) == Vector{Vector{Int64}}
    @test eltype(mapped) == Vector{Int64}

    runtime_literal = [collect((1, 2)), collect((3, 4))]
    @test runtime_literal == [[1, 2], [3, 4]]
    @test typeof(runtime_literal) == Vector{Vector{Int64}}
    @test eltype(runtime_literal) == Vector{Int64}

    promoted_literal = [collect((1, 2)), collect((3.0, 4.0))]
    @test promoted_literal == [[1.0, 2.0], [3.0, 4.0]]
    @test typeof(promoted_literal) == Vector{Vector{Float64}}
    @test eltype(promoted_literal) == Vector{Float64}

    matrix_elements = Vector{Matrix{Float64}}(undef, 1)
    @test typeof(matrix_elements) == Vector{Matrix{Float64}}
    @test eltype(matrix_elements) == Matrix{Float64}
end

true
