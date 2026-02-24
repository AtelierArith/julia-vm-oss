# Test Rational{Int64} - Int arithmetic (Issue #1785)

using Test

@testset "Rational{Int64} - Int - rational number minus integer" begin
    r = 7 // 4
    result = r - 1  # (7/4) - 1 = 3/4
    @test isapprox((Float64(result.num) / Float64(result.den)), 0.75)
end

true  # Test passed
