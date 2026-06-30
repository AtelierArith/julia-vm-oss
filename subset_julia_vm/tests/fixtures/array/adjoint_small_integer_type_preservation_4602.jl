using Test

@testset "adjoint preserves small integer element types (#4018, #4602)" begin
    signed_row = adjoint(Int8[1, 2])
    @test eltype(signed_row) === Int8
    @test size(signed_row) == (1, 2)
    @test typeof(signed_row[1, 1]) === Int8
    @test signed_row[1, 1] == Int8(1)
    @test signed_row[1, 2] == Int8(2)

    unsigned_row = adjoint(UInt8[1, 2])
    @test eltype(unsigned_row) === UInt8
    @test size(unsigned_row) == (1, 2)
    @test typeof(unsigned_row[1, 1]) === UInt8
    @test unsigned_row[1, 1] == UInt8(1)
    @test unsigned_row[1, 2] == UInt8(2)

    signed_matrix = adjoint(reshape(Int8[1, 2, 3, 4], 2, 2))
    @test eltype(signed_matrix) === Int8
    @test size(signed_matrix) == (2, 2)
    @test typeof(signed_matrix[1, 2]) === Int8
    @test signed_matrix[1, 2] == Int8(2)
    @test signed_matrix[2, 1] == Int8(3)

    unsigned_matrix = adjoint(reshape(UInt8[1, 2, 3, 4], 2, 2))
    @test eltype(unsigned_matrix) === UInt8
    @test size(unsigned_matrix) == (2, 2)
    @test typeof(unsigned_matrix[1, 2]) === UInt8
    @test unsigned_matrix[1, 2] == UInt8(2)
    @test unsigned_matrix[2, 1] == UInt8(3)
end

true
