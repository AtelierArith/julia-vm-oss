# Issue #6272: interprocedural exception inference must obtain a pure-Julia Base
# callee's exception type from the pure-Julia reflection classification
# (`Base._classified_exception_type`) rather than walking the callee's body or
# relying on Rust-side `gcd`/`lcm` name special-cases. A user wrapper around
# `gcd`/`lcm` must therefore compose the SAME exception type the direct call
# reports, for every fixed-width integer width — matching upstream Julia.

using Test

w_gcd(a, b) = gcd(a, b)
w_lcm(a, b) = lcm(a, b)
w_gcd2(a, b) = w_gcd(a, b)            # user -> user -> Base gcd
w_mix(a, b) = gcd(a, b) + lcm(a, b)   # two Base callees composed
w_idx_gcd(v, i, b) = gcd(v[i], b)    # throwing argument (v[i]) + Base callee
w_idx_lcm(v, i, b) = lcm(v[i], b)

@testset "gcd/lcm exception inference owned by pure Julia (Issue #6272)" begin
    # Direct calls: fixed-width signed `gcd` can overflow at `abs(typemin)`;
    # unsigned never does; `lcm` of any fixed-width integer can overflow /
    # divide-by-zero. Matches upstream `Base.infer_exception_type`.
    @test Base.infer_exception_type(gcd, Tuple{Int8,Int8}) == OverflowError
    @test Base.infer_exception_type(gcd, Tuple{Int32,Int32}) == OverflowError
    @test Base.infer_exception_type(gcd, Tuple{Int128,Int128}) == OverflowError
    @test Base.infer_exception_type(gcd, Tuple{UInt64,UInt64}) == Union{}
    @test Base.infer_exception_type(lcm, Tuple{Int32,Int32}) == Union{DivideError,OverflowError}
    @test Base.infer_exception_type(lcm, Tuple{UInt32,UInt32}) == Union{DivideError,OverflowError}

    # User wrappers compose the SAME exception type via the pure-Julia
    # classification, without walking `gcd`/`lcm`'s self-recursive bodies (no
    # long-tail runtime). This is the interprocedural-layering fix.
    @test Base.infer_exception_type(w_gcd, Tuple{Int32,Int32}) == OverflowError
    @test Base.infer_exception_type(w_gcd, Tuple{UInt64,UInt64}) == Union{}
    @test Base.infer_exception_type(w_lcm, Tuple{Int32,Int32}) == Union{DivideError,OverflowError}
    @test Base.infer_exception_type(w_gcd2, Tuple{Int64,Int64}) == OverflowError
    @test Base.infer_exception_type(w_mix, Tuple{Int64,Int64}) == Union{DivideError,OverflowError}

    # A throwing argument (`v[i]` -> BoundsError) must NOT suppress the Base
    # callee's own exception: the call composes BOTH (Issue #6272 review).
    @test Base.infer_exception_type(w_idx_gcd, Tuple{Vector{Int64},Int64,Int64}) ==
          Union{BoundsError,OverflowError}
    @test Base.infer_exception_type(w_idx_lcm, Tuple{Vector{Int64},Int64,Int64}) ==
          Union{DivideError,BoundsError,OverflowError}

    # The `nothrow` effect is cleared for the throwing wrappers.
    @test Base.infer_effects(w_gcd, Tuple{Int64,Int64}).nothrow == false
    @test Base.infer_effects(w_lcm, Tuple{Int64,Int64}).nothrow == false
end

true
