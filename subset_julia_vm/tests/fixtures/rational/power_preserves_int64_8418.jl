using Test

@testset "Rational power preserves Int64 element type (Issue #8418)" begin
    r = (2 // 3) ^ 3

    @test r == 8 // 27
    @test typeof(r) === Rational{Int64}
    @test typeof(r.num) === Int64
    @test typeof(r.den) === Int64
    @test Float64(r.num) / Float64(r.den) == 8.0 / 27.0
end

true
