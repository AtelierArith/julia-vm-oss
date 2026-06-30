using Test

# Issue #3498: Type inference used to collapse primitive arithmetic to Int64
# for everything that wasn't Float16/32/64. The fix added the centralized
# `promote_numeric_value_types` helper used by `infer_expr_type`, so inline
# arithmetic for UInt8/UInt64/Int128/BigInt and Int+Float promotion now match
# Julia. This fixture pins the four cases from the issue body plus
# Int+Float promotion as a regression guard.

@testset "Same-width primitive arithmetic preserves type (Issue #3498)" begin
    @test typeof(UInt64(1) + UInt64(2)) === UInt64
    @test UInt64(1) + UInt64(2) == UInt64(3)

    @test typeof(UInt8(1) + UInt8(2)) === UInt8
    @test UInt8(1) + UInt8(2) == UInt8(3)

    @test typeof(BigInt(1) + BigInt(2)) === BigInt
    @test BigInt(1) + BigInt(2) == BigInt(3)

    @test typeof(Int128(1) + Int128(2)) === Int128
    @test Int128(1) + Int128(2) == Int128(3)
end

@testset "Int + Float promotion preserves Float type (Issue #3498)" begin
    @test typeof(1 + 1.0) === Float64
    @test 1 + 1.0 == 2.0
    @test typeof(1.0 + 1) === Float64
    @test typeof(Int8(1) + 1.0) === Float64
    @test typeof(UInt8(1) + 1.0) === Float64
end

true
