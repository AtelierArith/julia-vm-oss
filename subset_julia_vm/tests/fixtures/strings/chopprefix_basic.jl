# Test: chopprefix function - remove prefix
# Expected: "world"

using Test

@testset "chopprefix(s, prefix) - remove prefix" begin

    @test (chopprefix("hello world", "hello ")) == "world"
end

true  # Test passed
