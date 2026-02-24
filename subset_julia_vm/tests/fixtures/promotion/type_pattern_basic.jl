# Test: promote_type(Float64, Int64) returns Float64

using Test

@testset "promote_type(Float64, Int64) returns Float64 via Type{T} dispatch" begin
    result = promote_type(Float64, Int64)
    @test (result === Float64)
end

true  # Test passed
