# Test promote_rule with Float64 and Int64
# promote_rule(Float64, Int64) should return Float64

using Test

@testset "promote_type(Float64, Int64) === Float64" begin

    # promote_type returns the promoted type
    # For Float64 and Int64, the result should be Float64
    r = promote_type(Float64, Int64)
    @test (r === Float64)
end

true  # Test passed
