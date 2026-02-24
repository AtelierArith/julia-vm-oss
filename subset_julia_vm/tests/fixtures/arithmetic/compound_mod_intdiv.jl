# Compound assignment %= and ÷= operators (Issue #2145)

using Test

@testset "compound %= and ÷= assignment" begin
    # %= (modulo)
    x = 10
    x %= 3
    @test x == 1

    y = 17
    y %= 5
    @test y == 2

    # %= on Float64
    z = 10.5
    z %= 3.0
    @test z == 1.5

    # ÷= (integer division)
    a = 10
    a ÷= 3
    @test a == 3

    b = 17
    b ÷= 5
    @test b == 3

    # %= on array index
    arr = [10, 20, 30]
    arr[1] %= 3
    @test arr[1] == 1

    # ÷= on array index
    arr[2] ÷= 7
    @test arr[2] == 2
end

true
