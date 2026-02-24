# Test that length() returns Int type for Tuple

using Test

@testset "length() returns Int64 type for Tuple" begin
    t = (1, 2, 3, 4)
    len = length(t)
    @test (typeof(len) == Int64)
end

true  # Test passed
