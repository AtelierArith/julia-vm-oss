# <: subtype expression
# Int64 <: Number is true, Float64 <: Real is true, Int64 <: AbstractFloat is false
# Result: 1 + 1 + 0 = 2.0

using Test

@testset "<: subtype expression" begin
    result = 0.0
    result += (Int64 <: Number) ? 1.0 : 0.0
    result += (Float64 <: Real) ? 1.0 : 0.0
    result += (Int64 <: AbstractFloat) ? 1.0 : 0.0
    @test (result) == 2.0
end

true  # Test passed
