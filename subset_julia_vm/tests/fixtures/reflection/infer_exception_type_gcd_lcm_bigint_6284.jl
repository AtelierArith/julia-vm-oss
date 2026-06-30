# Issue #6284: `gcd`/`lcm` over `BigInt` must report `Any` (not `Union{}`).
# Unlike fixed-width integers — where `gcd` can overflow at `abs(typemin)`
# (`OverflowError`) and `lcm` can overflow / divide-by-zero
# (`Union{DivideError, OverflowError}`), see #6272 — `BigInt` `gcd`/`lcm`
# delegate to GMP via `ccall`, which the inferrer cannot prove `nothrow`.
# Upstream Julia therefore reports `Any` (and `nothrow == false`), for direct
# calls, mixed `BigInt`/fixed-width pairs (which promote to `BigInt`), and user
# wrappers that compose the same pure-Julia classification (#6272 / PR #6283).

using Test

w_gcd_big(a, b) = gcd(a, b)
w_lcm_big(a, b) = lcm(a, b)
w_gcd2_big(a, b) = w_gcd_big(a, b)          # user -> user -> Base gcd
w_mix_big(a, b) = gcd(a, b) + lcm(a, b)     # two Base callees composed

@testset "gcd/lcm over BigInt report Any (Issue #6284)" begin
    # Direct calls: any BigInt argument -> Any.
    @test Base.infer_exception_type(gcd, Tuple{BigInt,BigInt}) == Any
    @test Base.infer_exception_type(lcm, Tuple{BigInt,BigInt}) == Any
    # Mixed BigInt / fixed-width promote to BigInt -> Any (both argument orders).
    @test Base.infer_exception_type(gcd, Tuple{BigInt,Int64}) == Any
    @test Base.infer_exception_type(gcd, Tuple{Int64,BigInt}) == Any
    @test Base.infer_exception_type(lcm, Tuple{BigInt,Int32}) == Any

    # The `nothrow` effect is cleared (could throw anything).
    @test Base.infer_effects(gcd, Tuple{BigInt,BigInt}).nothrow == false
    @test Base.infer_effects(lcm, Tuple{BigInt,BigInt}).nothrow == false

    # User wrappers compose the SAME `Any` classification interprocedurally.
    @test Base.infer_exception_type(w_gcd_big, Tuple{BigInt,BigInt}) == Any
    @test Base.infer_exception_type(w_lcm_big, Tuple{BigInt,BigInt}) == Any
    @test Base.infer_exception_type(w_gcd2_big, Tuple{BigInt,BigInt}) == Any
    @test Base.infer_exception_type(w_mix_big, Tuple{BigInt,BigInt}) == Any
    @test Base.infer_exception_type(w_gcd_big, Tuple{BigInt,Int64}) == Any
    @test Base.infer_effects(w_gcd_big, Tuple{BigInt,BigInt}).nothrow == false

    # Regression guard: fixed-width integer widths stay precise (Issue #6272),
    # i.e. the BigInt arm must not widen them to Any.
    @test Base.infer_exception_type(gcd, Tuple{Int8,Int8}) == OverflowError
    @test Base.infer_exception_type(lcm, Tuple{UInt32,UInt32}) == Union{DivideError,OverflowError}
    @test Base.infer_exception_type(w_gcd_big, Tuple{Int32,Int32}) == OverflowError
end

true
