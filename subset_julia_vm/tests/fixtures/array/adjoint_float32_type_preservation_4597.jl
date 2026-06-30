using Test

@testset "adjoint preserves Float32 element type (#4018, #4597)" begin
    v = Float32[1, 2]
    row = adjoint(v)
    @test eltype(row) === Float32
    @test size(row) == (1, 2)
    @test typeof(row[1, 1]) === Float32
    @test typeof(row[1, 2]) === Float32
    @test row[1, 1] == Float32(1)
    @test row[1, 2] == Float32(2)

    A = reshape(Float32[1, 2, 3, 4], 2, 2)
    transposed = adjoint(A)
    @test eltype(transposed) === Float32
    @test size(transposed) == (2, 2)
    @test typeof(transposed[1, 1]) === Float32
    @test typeof(transposed[1, 2]) === Float32
    @test typeof(transposed[2, 1]) === Float32
    @test typeof(transposed[2, 2]) === Float32
    @test transposed[1, 1] == Float32(1)
    @test transposed[1, 2] == Float32(2)
    @test transposed[2, 1] == Float32(3)
    @test transposed[2, 2] == Float32(4)
end

true
