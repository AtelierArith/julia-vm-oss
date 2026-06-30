using Test

@testset "zero(::Array) preserves element type (Issue #4419)" begin
    ints = zero([1, 2, 3])
    @test typeof(ints) === Vector{Int64}
    @test ints[1] === Int64(0)
    @test ints[2] === Int64(0)
    @test ints[3] === Int64(0)

    bools = zero([true, false])
    @test typeof(bools) === Vector{Bool}
    @test bools[1] == false
    @test bools[2] == false

    floats = zero([1.0 2.0; 3.0 4.0])
    @test typeof(floats) === Matrix{Float64}
    @test size(floats) === (2, 2)
    @test floats[1, 1] === 0.0
    @test floats[2, 2] === 0.0
end

true
