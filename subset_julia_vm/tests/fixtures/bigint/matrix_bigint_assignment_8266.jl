using Test

@testset "Matrix{BigInt} assignment preserves BigInt" begin
    A = Matrix{BigInt}(undef, 1, 1)
    A[1, 1] = big(2)

    @test A[1, 1] == big(2)
    @test typeof(A[1, 1]) === BigInt
end

true
