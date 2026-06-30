# Test: Multi-dimensional getindex preserves index arity / kind (Issue #3529)
# Scalar index returns an element, while range/slice indexing returns an array.
using Test

function elem_of_matrix()
    m = reshape(collect(1:6), 2, 3)
    return m[1, 1]
end

function row_of_matrix()
    m = reshape(collect(1:6), 2, 3)
    return m[1, :]
end

function col_of_matrix()
    m = reshape(collect(1:6), 2, 3)
    return m[:, 1]
end

@testset "Multi-dim getindex inference" begin
    @test elem_of_matrix() == 1
    @test length(row_of_matrix()) == 3
    @test length(col_of_matrix()) == 2
end

true
