# UInt64 comparison must not route through Int64 (Issue #3566)
using Test

@testset "typemax(UInt64) comparisons" begin
    # The literal 0xffffffffffffffff is now a UInt64 (post-#3559).
    # typemax(UInt64) is also a UInt64. The comparison must compare as u64,
    # NOT route through Int64 (which would overflow for u64::MAX).
    @test typemax(UInt64) == 0xffffffffffffffff
    @test !(typemax(UInt64) != 0xffffffffffffffff)
    @test typemax(UInt64) >= 0xffffffffffffffff
    @test typemax(UInt64) <= 0xffffffffffffffff
    @test !(typemax(UInt64) < 0xffffffffffffffff)
    @test !(typemax(UInt64) > 0xffffffffffffffff)
end

@testset "Large UInt64 vs smaller UInt64" begin
    a = typemax(UInt64)
    b = UInt64(0)
    @test a > b
    @test a >= b
    @test b < a
    @test b <= a
    @test a != b
    @test !(a == b)
end

@testset "UInt64 ordering at boundary" begin
    big = UInt64(0xffffffffffffffff)
    one = UInt64(1)
    zero = UInt64(0)
    @test big > one
    @test big > zero
    @test one > zero
    @test !(zero > one)
    @test big - one < big
end

true
