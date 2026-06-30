using Test

# Issue #3621: Int128 arithmetic must preserve Int128, not widen to BigInt.
# Inline expressions like `Int128(1) + Int128(2)` previously hit the BigInt
# early-route in compile/expr/binary/mod.rs and produced BigInt instead of Int128.
@testset "Int128 type preservation (Issue #3621)" begin
    # Inline arithmetic preserves Int128
    @test typeof(Int128(1) + Int128(2)) == Int128
    @test typeof(Int128(5) - Int128(3)) == Int128
    @test typeof(Int128(3) * Int128(4)) == Int128
    @test Int128(1) + Int128(2) == Int128(3)
    @test Int128(3) * Int128(4) == Int128(12)

    # Variable-bound arithmetic also preserves Int128
    x = Int128(10)
    y = Int128(7)
    @test typeof(x + y) == Int128
    @test typeof(x - y) == Int128
    @test typeof(x * y) == Int128
    @test x + y == Int128(17)

    # Mixed Int128 + Int64 stays Int128 (Julia: signed promotion)
    @test typeof(Int128(1) + 2) == Int128
    @test typeof(2 + Int128(1)) == Int128
    @test Int128(1) + 2 == Int128(3)

    # Division of Int128 / Int128 returns Float64 (Julia's `/` always floats)
    @test typeof(Int128(10) / Int128(3)) == Float64

    # Float promotion: Int128 + Float64 -> Float64, etc.
    @test typeof(Int128(1) + 1.0) == Float64
    @test typeof(Int128(1) + Float32(1.0)) == Float32
    @test typeof(Int128(1) + Float16(1.0)) == Float16

    # Comparisons return Bool
    @test (Int128(1) == Int128(1)) === true
    @test (Int128(1) < Int128(2)) === true
    @test (Int128(2) > Int128(1)) === true

    # BigInt + Int128 stays BigInt (BigInt early-route still wins)
    @test typeof(BigInt(1) + Int128(2)) == BigInt
    @test typeof(Int128(1) + BigInt(2)) == BigInt
end

true
