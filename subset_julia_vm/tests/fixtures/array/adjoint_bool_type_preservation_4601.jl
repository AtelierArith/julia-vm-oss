using Test

@testset "adjoint preserves Bool element type (#4018, #4601)" begin
    v = Bool[true, false]
    row = adjoint(v)
    @test eltype(row) === Bool
    @test size(row) == (1, 2)
    @test typeof(row[1, 1]) === Bool
    @test typeof(row[1, 2]) === Bool
    @test row[1, 1] === true
    @test row[1, 2] === false

    A = reshape(Bool[true, false, true, false], 2, 2)
    transposed = adjoint(A)
    @test eltype(transposed) === Bool
    @test size(transposed) == (2, 2)
    @test typeof(transposed[1, 1]) === Bool
    @test typeof(transposed[1, 2]) === Bool
    @test typeof(transposed[2, 1]) === Bool
    @test typeof(transposed[2, 2]) === Bool
    @test transposed[1, 1] === true
    @test transposed[1, 2] === false
    @test transposed[2, 1] === true
    @test transposed[2, 2] === false
end

true
