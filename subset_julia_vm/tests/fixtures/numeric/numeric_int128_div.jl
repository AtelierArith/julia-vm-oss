using Test

# Issue #3694: `÷` (lowered to `div`) on two Int128s previously fell to the
# generic `div(x, y) = floor(x / y)` which widened to Float64. With a Pure
# Julia `div(::Int128, ::Int128)` backed by an I128-aware `sdiv_int`
# intrinsic, the result stays Int128 — including for values outside the
# Int64 range.
@testset "Int128 div preservation (Issue #3694)" begin
    # Type preservation
    @test typeof(Int128(10) ÷ Int128(3)) == Int128
    @test typeof(div(Int128(10), Int128(3))) == Int128

    # Numerical correctness for in-range values
    @test Int128(10) ÷ Int128(3) == Int128(3)
    @test Int128(-10) ÷ Int128(3) == Int128(-3)
    @test div(Int128(10), Int128(3)) == Int128(3)

    # Values that exceed Int64 — must not silently truncate
    big_val = Int128(typemax(Int64)) * Int128(3)
    @test typeof(big_val ÷ Int128(3)) == Int128
    @test big_val ÷ Int128(3) == Int128(typemax(Int64))
    @test big_val ÷ big_val == Int128(1)

    # Division by negative
    @test Int128(20) ÷ Int128(-7) == Int128(-2)
end

true
