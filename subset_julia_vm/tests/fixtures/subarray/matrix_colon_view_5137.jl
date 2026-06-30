using Test

@testset "Matrix colon SubArray view (Issue #5137)" begin
    A = reshape(collect(1:9), 3, 3)

    row_view = view(A, 1:2, :)
    @test occursin("SubArray{Int64, 2, Matrix{Int64}", string(typeof(row_view)))
    @test row_view isa AbstractArray{Int64,2}
    @test row_view isa AbstractMatrix{Int64}
    @test size(row_view) == (2, 3)
    @test length(row_view) == 6
    @test ndims(row_view) == 2
    @test parent(row_view) === A
    row_parent_indices = parentindices(row_view)
    @test row_parent_indices[1] == 1:2
    @test length(row_parent_indices[2]) == 3
    @test row_parent_indices[2][1] == 1
    @test row_parent_indices[2][3] == 3
    @test row_view[2, 3] == 8

    row_view[1, 2] = 99
    @test A[1, 2] == 99
    A[2, 3] = 77
    @test row_view[2, 3] == 77
    row_copy = collect(row_view)
    @test row_copy == [1 99 7; 2 5 77]
    @test typeof(row_copy) == Matrix{Int64}
    @test size(row_copy) == (2, 3)

    col_view = view(A, :, 2:3)
    @test occursin("SubArray{Int64, 2, Matrix{Int64}", string(typeof(col_view)))
    @test col_view isa AbstractArray{Int64,2}
    @test col_view isa AbstractMatrix{Int64}
    @test size(col_view) == (3, 2)
    col_parent_indices = parentindices(col_view)
    @test length(col_parent_indices[1]) == 3
    @test col_parent_indices[1][1] == 1
    @test col_parent_indices[1][3] == 3
    @test col_parent_indices[2] == 2:3
    @test col_view[1, 1] == 99

    col_view[3, 2] = 88
    @test A[3, 3] == 88
    col_copy = collect(col_view)
    @test col_copy == [99 7; 5 77; 6 88]
    @test typeof(col_copy) == Matrix{Int64}
    @test size(col_copy) == (3, 2)

    full_view = view(A, :, :)
    @test size(full_view) == (3, 3)
    @test parent(full_view) === A
    @test full_view[3, 3] == 88
    full_view[2, 1] = 66
    @test A[2, 1] == 66
    full_copy = collect(full_view)
    @test full_copy == [1 99 7; 66 5 77; 3 6 88]
    @test typeof(full_copy) == Matrix{Int64}
end

true
