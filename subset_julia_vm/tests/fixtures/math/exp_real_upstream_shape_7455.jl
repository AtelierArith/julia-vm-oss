# Regression coverage for upstream-shaped exp(::Real) behavior.
#
# The subnormal boundary and exact Float64 bit patterns below were verified
# against upstream Julia 1.12.6.
using Test

@testset "exp Real dispatch and edge values (Issues #7455/#7484)" begin
    @test reinterpret(UInt64, exp(-745.0)) == 0x0000000000000001
    @test exp(-745.1332191019412) === 0.0
    @test abs(exp(1.0) - 2.718281828459045) <= 1.0e-15

    @test typeof(exp(Float32(1))) === Float32
    @test exp(Int64(1)) === exp(1.0)
    @test exp(true) === exp(1.0)
    @test exp(false) === 1.0
    @test exp(1//2) === exp(0.5)
end

true
