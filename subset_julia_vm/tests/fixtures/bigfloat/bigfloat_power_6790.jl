using Test

# Issue #6790: BigFloat ^ <real numeric> previously infinite-recursed (no
# terminating `^(::BigFloat, …)` method existed, so runtime `^` dispatch
# re-entered itself → stack overflow). It is now computed inline as a BigFloat
# power (astro_float pow), matching upstream Julia.

@testset "BigFloat integer power (Issue #6790)" begin
    @test big(2.0)^10 == 1024
    @test big(2.0)^3 == 8
    @test big(2.0)^0 == 1
    @test big(2.0)^1 == 2
    @test big(2.0)^(-2) == 0.25
    @test big(2.0)^100 == big(2)^100
    @test typeof(big(2.0)^10) === BigFloat
    @test typeof(big(2.0)^100) === BigFloat
end

# Integer-VALUED float / BigFloat exponents also resolve to BigFloat (they take
# astro_float's power-by-squaring route). Genuinely fractional exponents
# (e.g. ^0.5) are tracked separately (#6790 covers the integer-exponent crash).
@testset "BigFloat integer-valued float exponent (Issue #6790)" begin
    @test big(2.0)^3.0 == 8
    @test big(2.0)^big(3.0) == 8
    @test big(2.0)^big(3) == 8
    @test big(10.0)^6.0 == 1000000
    @test typeof(big(2.0)^3.0) === BigFloat
    @test typeof(big(2.0)^big(3.0)) === BigFloat
end

@testset "BigFloat power in a loop (Issue #6790)" begin
    acc = big(0.0)
    for k in 0:5
        acc += big(2.0)^k
    end
    @test acc == 63   # 1+2+4+8+16+32
end

true
