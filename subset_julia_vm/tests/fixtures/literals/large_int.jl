# Large integer literal tests

using Test

@testset "Int128 and BigInt literals for values exceeding Int64 range" begin

    # Int128 literals (values > i64::MAX)
    @assert typeof(9223372036854775808) == Int128
    @assert 9223372036854775808 > 0

    # Verify Int64 boundary
    @assert typeof(9223372036854775807) == Int64
    @assert 9223372036854775807 == Int64(9223372036854775807)

    # BigInt literals (values > i128::MAX)
    @assert typeof(170141183460469231731687303715884105728) == BigInt

    @test (1.0) == 1.0
end

true  # Test passed
