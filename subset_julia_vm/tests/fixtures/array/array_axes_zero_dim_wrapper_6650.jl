using Test

@testset "Array wrapper axes and zero-dimensional indexing (#6650)" begin
    scalar = Array{Int64,0}(undef, ())
    setindex!(scalar, 42)

    @test axes(scalar) == ()
    @test axes(scalar, 1) == Base.OneTo(1)
    @test getindex(scalar) == 42
    @test setindex!(scalar, 7) === scalar
    @test getindex(scalar) == 7

    vector = [1, 2, 3]
    vector_axes = axes(vector)
    # Compatibility note (#6685): compare tuple axes component-wise until tuple
    # equality over OneTo struct elements matches upstream Julia.
    @test length(vector_axes) == 1
    @test first(vector_axes[1]) == 1
    @test last(vector_axes[1]) == 3
    @test length(vector_axes[1]) == 3
    @test axes(vector, 1) == Base.OneTo(3)
    @test axes(vector, 2) == Base.OneTo(1)

    matrix = [1 2; 3 4]
    matrix_axes = axes(matrix)
    @test length(matrix_axes) == 2
    @test first(matrix_axes[1]) == 1
    @test last(matrix_axes[1]) == 2
    @test length(matrix_axes[1]) == 2
    @test first(matrix_axes[2]) == 1
    @test last(matrix_axes[2]) == 2
    @test length(matrix_axes[2]) == 2
    @test axes(matrix, 1) == Base.OneTo(2)
    @test axes(matrix, 2) == Base.OneTo(2)
    @test axes(matrix, 3) == Base.OneTo(1)
end

true
