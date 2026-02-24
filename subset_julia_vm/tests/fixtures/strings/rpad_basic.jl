# Test: rpad function - right pad string
# Expected: "abc  " (5 chars total)

using Test

@testset "rpad(s, n) - right pad string" begin

    @test (rpad("abc", 5)) == "abc  "
end

true  # Test passed
