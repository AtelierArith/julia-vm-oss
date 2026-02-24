# Test: chopsuffix function - remove suffix
# Expected: "hello"

using Test

@testset "chopsuffix(s, suffix) - remove suffix" begin

    @test (chopsuffix("hello world", " world")) == "hello"
end

true  # Test passed
