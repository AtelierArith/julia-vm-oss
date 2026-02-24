# Test: lpad function - left pad string
# Expected: "  abc" (5 chars total)

using Test

@testset "lpad(s, n) - left pad string" begin

    @test (lpad("abc", 5)) == "  abc"
end

true  # Test passed
