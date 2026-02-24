# Test isunordered function

using Test

@testset "isunordered - check if value is unordered (NaN, Missing)" begin

    # NaN is unordered
    @assert isunordered(NaN)

    # Missing is unordered
    @assert isunordered(missing)

    # Regular values are ordered
    @assert !isunordered(1)
    @assert !isunordered(3.14)
    @assert !isunordered(0.0)
    @assert !isunordered(Inf)
    @assert !isunordered(-Inf)

    @test (true)
end

true  # Test passed
