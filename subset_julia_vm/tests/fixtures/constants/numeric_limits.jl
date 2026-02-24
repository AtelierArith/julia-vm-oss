using Test

@testset "numeric type limits and special values" begin
    @test typemax(Int64) == 9223372036854775807
    # Use computation to avoid unary negation literal parsing
    @test typemin(Int64) == 0 - 9223372036854775807 - 1
    @test Inf > typemax(Int64)
    neg_inf = typemin(Float64)
    @test neg_inf < typemin(Int64)
    @test isnan(NaN)
    @test !isnan(Inf)
    @test isinf(Inf)
    @test isinf(neg_inf)
    @test !isinf(NaN)

    # UInt64 typemax/typemin (Issue #3151)
    @test typemax(UInt64) == 0xffffffffffffffff
    @test typemin(UInt64) == UInt64(0)

    # typeof preserves UInt64 type for typemin (Issue #3151)
    @test typeof(typemin(UInt64)) == UInt64
end

true
