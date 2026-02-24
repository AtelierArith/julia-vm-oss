# Bare comma return syntax: return a, b => return (a, b) (Issue #2076)

using Test

function swap(a, b)
    return b, a
end

function triple(x)
    return x, x * 2, x * 3
end

function compute(x, y)
    return x + y, x - y, x * y
end

@testset "Bare comma return (Issue #2076)" begin
    # Two values
    x, y = swap(1, 2)
    @test x == 2
    @test y == 1

    # Three values
    a, b, c = triple(5)
    @test a == 5
    @test b == 10
    @test c == 15

    # Expressions in return
    s, d, p = compute(10, 3)
    @test s == 13
    @test d == 7
    @test p == 30
end

true
