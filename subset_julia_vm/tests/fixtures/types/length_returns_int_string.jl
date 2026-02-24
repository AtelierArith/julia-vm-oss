# Test that length() returns Int type for String

using Test

@testset "length() returns Int64 type for String" begin
    s = "hello"
    len = length(s)
    @test (typeof(len) == Int64)
end

true  # Test passed
