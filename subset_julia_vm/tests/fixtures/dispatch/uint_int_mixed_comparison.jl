# UInt8/Int64 mixed-type comparison and arithmetic (Issue #1853)
# Tests that UInt types can be compared and promoted with Int types

using Test

@testset "UInt8 mixed-type comparison" begin
    # UInt8 == Int64
    x = UInt8(72)
    @test x == 72
    @test 72 == x

    # UInt8 != Int64
    @test x != 73
    @test 73 != x

    # UInt8 < Int64
    @test x < 100
    @test !(x < 72)

    # UInt8 > Int64
    @test x > 50
    @test !(x > 72)

    # UInt8 <= Int64
    @test x <= 72
    @test x <= 100

    # UInt8 >= Int64
    @test x >= 72
    @test x >= 50
end

@testset "UInt8 promotion rules" begin
    # promote_type with explicit Type arguments
    @test promote_type(Int64, UInt8) == Int64
    @test promote_type(UInt8, Int64) == Int64
    @test promote_type(Int64, UInt16) == Int64
    @test promote_type(Int64, UInt32) == Int64
    @test promote_type(Int128, UInt64) == Int128

    # promote_rule direct
    @test promote_rule(Int64, UInt8) == Int64
    @test promote_rule(Int32, UInt8) == Int32
    @test promote_rule(Int16, UInt8) == Int16
end

@testset "UInt8 arithmetic with Int64" begin
    x = UInt8(10)
    y = 20

    # Addition
    @test x + y == 30

    # Subtraction
    @test y - x == 10

    # Multiplication
    @test x * y == 200
end

true
