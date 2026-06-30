using Test

struct TraitLengthIter4052 end

function Base.iterate(::TraitLengthIter4052)
    return (1, 2)
end

function Base.iterate(::TraitLengthIter4052, state)
    if state > 3
        return nothing
    end
    return (state, state + 1)
end

Base.length(::TraitLengthIter4052) = 3
Base.eltype(::TraitLengthIter4052) = Int64
Base.IteratorSize(::TraitLengthIter4052) = Base.HasLength()
Base.IteratorEltype(::TraitLengthIter4052) = Base.HasEltype()

@testset "collect _similar_for trait allocation (Issue #4052)" begin
    itr = TraitLengthIter4052()
    length_values = Base._collect([0], itr, Base.IteratorEltype(itr), Base.IteratorSize(itr))
    @test typeof(length_values) === Vector{Int64}
    @test eltype(length_values) === Int64
    @test length(length_values) == 3
    @test length_values[1] == 1
    @test length_values[2] == 2
    @test length_values[3] == 3

    matrix = [1 2; 3 4]
    shape_values = Base._collect([0 0; 0 0], matrix, Base.IteratorEltype(matrix), Base.IteratorSize(matrix))
    @test typeof(shape_values) === Matrix{Int64}
    @test eltype(shape_values) === Int64
    @test size(shape_values) == (2, 2)
    @test shape_values[1, 1] == 1
    @test shape_values[2, 1] == 3
    @test shape_values[1, 2] == 2
    @test shape_values[2, 2] == 4
    shape_values[1, 1] = 99
    @test matrix[1, 1] == 1

    tuple_values = Base._collect([0.0], (1, 2.0), Base.EltypeUnknown(), Base.HasLength())
    @test typeof(tuple_values) === Vector{Real}
    @test eltype(tuple_values) === Real
    @test length(tuple_values) == 2
    @test tuple_values[1] == 1
    @test tuple_values[2] == 2.0

    unknown_shape_values = Base._collect([0 0; 0 0], matrix, Base.EltypeUnknown(), Base.IteratorSize(matrix))
    @test typeof(unknown_shape_values) === Matrix{Int64}
    @test eltype(unknown_shape_values) === Int64
    @test size(unknown_shape_values) == (2, 2)
    @test unknown_shape_values[1, 1] == 1
    @test unknown_shape_values[2, 1] == 3
    @test unknown_shape_values[1, 2] == 2
    @test unknown_shape_values[2, 2] == 4

    typed_matrix = collect(Float64, matrix)
    @test typeof(typed_matrix) === Matrix{Float64}
    @test eltype(typed_matrix) === Float64
    @test size(typed_matrix) == (2, 2)
    @test typed_matrix[1, 1] == 1.0
    @test typed_matrix[2, 2] == 4.0
end

true
