using Test

# Issue #5679: `gcd`/`lcm` accept a collection or 3+ arguments, not just two.
# `gcd([12,18,24]) == 6`, `gcd(12,18,24) == 6`; empty returns the identity
# (gcd -> 0, lcm -> 1). sjulia only had the 2-argument methods.

@testset "gcd/lcm over an array (Issue #5679)" begin
    @test gcd([12, 18, 24]) == 6
    @test lcm([2, 3, 4]) == 12
    @test gcd([10]) == 10
    @test lcm([7]) == 7
    @test gcd(Int[]) == 0       # identity
    @test lcm(Int[]) == 1       # identity
    @test gcd([17, 5]) == 1
    @test lcm([6, 10, 15]) == 30
end

@testset "gcd/lcm with 3+ arguments (Issue #5679)" begin
    @test gcd(12, 18, 24) == 6
    @test lcm(2, 3, 4) == 12
    @test gcd(12, 18, 24, 6) == 6
    @test lcm(2, 3, 4, 5) == 60
end

@testset "two-argument gcd/lcm unchanged (Issue #5679)" begin
    @test gcd(48, 36) == 12
    @test lcm(4, 6) == 12
    @test gcd(0, 5) == 5
end

true
