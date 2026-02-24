using Test

@testset "anonymous functions (lambdas)" begin
    # Basic lambda
    f = x -> x^2
    @test f(3) == 9
    @test f(4) == 16

    # Multi-arg lambda
    g = (x, y) -> x + y
    @test g(3, 4) == 7

    # Lambda stored and called multiple times
    adder = x -> x + 100
    @test adder(1) == 101
    @test adder(50) == 150

    # Higher-order: map with lambda
    nums = [1, 2, 3, 4]
    doubled = map(x -> x * 2, nums)
    @test doubled == [2, 4, 6, 8]

    # Filter with lambda
    evens = filter(x -> x % 2 == 0, [1, 2, 3, 4, 5, 6])
    @test evens == [2, 4, 6]

    # Immediately invoked lambda: (x -> expr)(arg) (Issue #3142)
    @test (x -> x * 2)(5) == 10
    @test (x -> x^2)(4) == 16
    @test ((x, y) -> x + y)(3, 4) == 7

    # IIFE with closure over outer variable (Issue #3149)
    n = 10
    @test (x -> x + n)(5) == 15

    # Chained IIFE: nested immediately invoked lambdas (Issue #3149)
    @test (x -> (y -> x + y)(3))(5) == 8
end

true
