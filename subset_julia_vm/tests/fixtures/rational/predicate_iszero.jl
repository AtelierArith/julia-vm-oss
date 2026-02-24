# Test iszero predicate for Rational

using Test

@testset "iszero predicate for Rational" begin
    r1 = 0 // 5
    r2 = 1 // 5
    result = 0.0
    if iszero(r1)
        result = result + 1.0
    end
    if iszero(r2)
        result = result + 10.0
    end
    @test (result) == 1.0
end

true  # Test passed
