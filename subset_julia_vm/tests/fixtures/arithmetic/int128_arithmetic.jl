# Int128 arithmetic operations (Issue #1904)

using Test

@testset "Int128 arithmetic" begin
    a = Int128(100)
    b = Int128(200)

    # Basic arithmetic
    @test a + b == Int128(300)
    @test b - a == Int128(100)
    @test a * b == Int128(20000)

    # Division (/ returns Float64 in Julia)
    @test a / b == 0.5

    # Comparison operators
    @test a < b
    @test b > a
    @test a <= b
    @test b >= a
    @test a == Int128(100)
    @test a != b

    # Mixed Int128-Int64 arithmetic (promotes to Int128)
    x = Int128(10)
    y = 3  # Int64
    @test x + y == Int128(13)
    @test x - y == Int128(7)
    @test x * y == Int128(30)

    # Mixed Int128-Int64 comparison
    @test x > y
    @test x >= y
    @test y < x
    @test y <= x

end

true
