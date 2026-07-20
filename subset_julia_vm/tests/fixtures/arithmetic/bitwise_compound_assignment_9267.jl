# Bitwise compound assignment operators (Issue #9267)

using Test

mutable struct BitwiseCompoundBox9267
    value::Int
end

@testset "bitwise compound assignment operators (Issue #9267)" begin
    x = true
    x &= false
    @test x === false

    y = false
    y |= true
    @test y === true

    z = 12
    z ⊻= 1
    @test z == 13

    r = 12
    r >>= 1
    @test r == 6

    l = 12
    l <<= 1
    @test l == 24

    a = [0b1100]
    a[1] &= 0b1010
    @test a[1] == 0b1000

    box = BitwiseCompoundBox9267(12)
    box.value <<= 1
    @test box.value == 24
end

true
