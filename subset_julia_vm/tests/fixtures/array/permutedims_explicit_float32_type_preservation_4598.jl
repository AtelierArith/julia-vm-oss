using Test

@testset "explicit permutedims preserves Float32 element type (#4018, #4598)" begin
    A = reshape(Float32[1, 2, 3, 4], 2, 2)
    identity_copy = permutedims(A, (1, 2))
    @test typeof(identity_copy) === Matrix{Float32}
    @test eltype(identity_copy) === Float32
    @test size(identity_copy) == (2, 2)
    @test typeof(identity_copy[1, 1]) === Float32
    @test identity_copy[1, 1] == Float32(1)
    @test identity_copy[2, 2] == Float32(4)

    B = reshape(Float32[1, 2, 3, 4, 5, 6, 7, 8], 2, 2, 2)
    permuted3 = permutedims(B, (2, 1, 3))
    @test typeof(permuted3) === Array{Float32,3}
    @test eltype(permuted3) === Float32
    @test size(permuted3) == (2, 2, 2)
    @test typeof(permuted3[1, 1, 1]) === Float32
    @test permuted3[1, 1, 1] == Float32(1)
    @test permuted3[1, 2, 1] == Float32(2)
    @test permuted3[2, 1, 2] == Float32(7)

    C = reshape(Float32[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16], 2, 2, 2, 2)
    permuted4 = permutedims(C, (2, 1, 3, 4))
    @test typeof(permuted4) === Array{Float32,4}
    @test eltype(permuted4) === Float32
    @test size(permuted4) == (2, 2, 2, 2)
    @test typeof(permuted4[1, 1, 1, 1]) === Float32
    @test permuted4[1, 1, 1, 1] == Float32(1)
    @test permuted4[1, 2, 1, 1] == Float32(2)
    @test permuted4[2, 1, 2, 2] == Float32(15)
end

true
