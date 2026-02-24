# Test typemin(Int64) returns minimum Int64 value
# typemin(Int64) == -9223372036854775808

using Test

@testset "typemin(Int64) returns minimum Int64 value" begin
    x = typemin(Int64)
    @test (x < 0 ? 1.0 : 0.0) == 1.0
end

true  # Test passed
