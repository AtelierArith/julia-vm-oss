using Test

# Issue #11214: checked integer constructors (Int64(::AbstractFloat),
# Int64(::BigInt), and siblings) must raise InexactError for every
# out-of-range value, including a float that is exactly integer-valued but
# outside the target range, and an out-of-range BigInt.
#
# Two boundary defects previously escaped this check:
#   1. `Float64(2.0^63)` silently saturated to `typemax(Int64)` instead of
#      raising `InexactError`. Root cause: the old check cast the float to
#      the target type (a SATURATING cast in Rust), then cast the saturated
#      result back to float and compared -- but `typemax(Int64)` is not
#      itself exactly representable in `Float64`; the nearest representable
#      value is `2.0^63`, which is exactly the out-of-range input. The
#      round-trip therefore falsely matched.
#   2. An out-of-range `BigInt` raised `TypeError` rather than `InexactError`.
#
# Every assertion below is verified against upstream julia 1.12.6.
@testset "numeric int range conversion boundary 11214" begin
    # --- Float64(2^63): out of Int64 range, must raise InexactError, not
    # silently saturate to typemax(Int64). ---
    @test_throws InexactError Int64(9223372036854775808.0)
    @test_throws InexactError Int64(9223372036854775808.0f0)

    # --- -2^63 IS typemin(Int64) and remains a VALID conversion (lower bound
    # is inclusive; only the upper bound at 2^63 is out of range). ---
    @test Int64(-9223372036854775808.0) === typemin(Int64)
    @test Int64(-9223372036854775808.0f0) === typemin(Int64)

    # --- BigInt out of Int64 range: must raise InexactError, not TypeError. ---
    @test_throws InexactError Int64(big"9223372036854775808")
    @test_throws InexactError Int64(big"-9223372036854775809")

    # --- BigInt at the Int64 boundary: still valid. ---
    @test Int64(big"9223372036854775807") === typemax(Int64)
    @test Int64(big"-9223372036854775808") === typemin(Int64)

    # --- Same defect shape, one width down: Float32(2^31) is exactly
    # representable in Float32 but out of Int32 range. ---
    @test_throws InexactError Int32(Float32(2147483648.0))
    @test Int32(Float32(-2147483648.0)) === typemin(Int32)

    # --- UInt64(2^64): out of UInt64 range. ---
    @test_throws InexactError UInt64(18446744073709551616.0)
    @test UInt64(0.0) === UInt64(0)

    # --- Large in-range values must still convert exactly (the tightened
    # boundary must not perturb accepted conversions). ---
    @test Int128(2.0^100) === Int128(2)^100
    @test UInt128(2.0^100) === UInt128(2)^100
end

true
