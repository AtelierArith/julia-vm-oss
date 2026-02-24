# Test ifelse function - conditional without short-circuit

using Test

@testset "ifelse - conditional without short-circuit" begin

    # ifelse(true, x, y) returns x
    @assert ifelse(true, 1, 2) == 1

    # ifelse(false, x, y) returns y
    @assert ifelse(false, 1, 2) == 2

    # Both branches are evaluated (unlike ternary)
    a = 10
    b = 20
    result = ifelse(a < b, a + 1, b + 1)
    @assert result == 11

    @test (true)
end

true  # Test passed
