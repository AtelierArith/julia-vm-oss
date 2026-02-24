# Test Rational{Int64} / Int arithmetic (Issue #1785)

using Test

@testset "Rational{Int64} / Int - rational number divided by integer" begin
    r = 3 // 4
    result = r / 2  # (3/4) / 2 = 3/8
    @test isapprox((Float64(result.num) / Float64(result.den)), 0.375)
end

true  # Test passed
