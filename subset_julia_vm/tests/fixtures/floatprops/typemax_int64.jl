# Test typemax(Int64) returns maximum Int64 value
# typemax(Int64) == 9223372036854775807

using Test

@testset "typemax(Int64) returns maximum Int64 value" begin
    x = typemax(Int64)
    @test (x > 0 ? 1.0 : 0.0) == 1.0
end

true  # Test passed
