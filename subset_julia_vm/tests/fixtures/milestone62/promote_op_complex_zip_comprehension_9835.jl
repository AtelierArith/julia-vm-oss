using Test

@testset "Complex promote_op and comprehension eltype" begin
    @test Base.promote_op(+, Complex{Int64}, Float64) == ComplexF64

    zs = [complex(1, 2)]
    rs = [0.5]
    sums = [z + r for (z, r) in zip(zs, rs)]
    @test typeof(sums) == Vector{ComplexF64}
    @test typeof(sums[1]) == ComplexF64
    @test real(sums[1]) == 1.5
    @test imag(sums[1]) == 2.0
end

true
