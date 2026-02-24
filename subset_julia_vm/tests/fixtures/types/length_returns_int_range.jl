# Test that length() returns Int type for Range

using Test

@testset "length() returns Int64 type for Range" begin
    r = 1:10
    len = length(r)
    @test (typeof(len) == Int64)
end

true  # Test passed
