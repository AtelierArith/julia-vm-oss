# Issue #4789: abs(typemin(IntN)) for N < 64 crashed with
# "Type error: Cannot negate" instead of returning typemin(IntN)
# itself. Upstream Julia silently returns the same value because
# two's-complement arithmetic wraps: -typemin(IntN) == typemin(IntN)
# for any signed N-bit integer.
#
# Pure-Julia abs(x::Int64) = flipsign(x, x) handled the Int64 case
# correctly, but narrow signed integers fell through to the generic
# abs(x) = -x if signbit(x) else x in base/number.jl, which routed
# through dynamic_neg whose narrow-integer arms were missing.
#
# Fix: subset_julia_vm/src/vm/dynamic_ops/mod.rs::dynamic_neg now
# uses wrapping_neg for all signed integer widths (I8/I16/I32/I64/
# I128), matching upstream's two's-complement semantics.

using Test

@testset "abs(typemin(IntN)) returns typemin (Issue #4789)" begin
    @test abs(Int8(-128)) === Int8(-128)
    @test abs(Int16(-32768)) === Int16(-32768)
    @test abs(typemin(Int32)) === typemin(Int32)
    @test abs(typemin(Int64)) === typemin(Int64)
    @test abs(typemin(Int128)) === typemin(Int128)
end

@testset "abs of normal narrow signed integers unchanged (Issue #4789)" begin
    @test abs(Int8(-5)) === Int8(5)
    @test abs(Int8(5)) === Int8(5)
    @test abs(Int8(0)) === Int8(0)
    @test abs(Int16(-100)) === Int16(100)
    @test abs(Int32(-1)) === Int32(1)
end

@testset "unary minus also wraps narrow typemin (Issue #4789)" begin
    # The underlying primitive: -typemin(IntN) === typemin(IntN)
    @test -Int8(-128) === Int8(-128)
    @test -Int16(-32768) === Int16(-32768)
    @test -typemin(Int32) === typemin(Int32)
end

@testset "Float / Bool negation regression guard (Issue #4789)" begin
    # Make sure the dynamic_neg fix doesn't regress non-integer arms
    @test -3.5 === -3.5
    @test -Float32(2.0) === Float32(-2.0)
    @test -true === -1
    @test -false === 0
end

true
