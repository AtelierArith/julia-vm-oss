# First-class functions test
# Regression test for #1457: Functions cannot be passed as arguments and called
# This tests that functions can be treated as first-class values

using Test

# Define test functions outside @testset (required by SubsetJuliaVM)
add(a, b) = a + b
mul(a, b) = a * b
sub(a, b) = a - b

# Higher-order function that takes a function and calls it with arguments
caller(f, x, y) = f(x, y)

# Function with 3 arguments
sum3(a, b, c) = a + b + c
caller3(f, x, y, z) = f(x, y, z)

# Nested higher-order functions
apply_twice(f, x, y) = f(f(x, y), y)

@testset "First-class functions" begin
    @testset "Basic function passing" begin
        # Pass add function as argument
        @test caller(add, 1, 2) == 3
        @test caller(add, 10, 20) == 30

        # Pass mul function as argument
        @test caller(mul, 3, 4) == 12
        @test caller(mul, 5, 6) == 30

        # Pass sub function as argument
        @test caller(sub, 10, 3) == 7
    end

    @testset "Function with more than 2 arguments" begin
        @test caller3(sum3, 1, 2, 3) == 6
        @test caller3(sum3, 10, 20, 30) == 60
    end

    @testset "Nested function calls" begin
        # apply_twice(add, 2, 3) = add(add(2, 3), 3) = add(5, 3) = 8
        @test apply_twice(add, 2, 3) == 8

        # apply_twice(mul, 2, 3) = mul(mul(2, 3), 3) = mul(6, 3) = 18
        @test apply_twice(mul, 2, 3) == 18
    end

    @testset "Anonymous functions passed as arguments" begin
        # Pass anonymous function (lambda)
        @test caller((x, y) -> x + y, 5, 7) == 12
        @test caller((x, y) -> x * y, 4, 5) == 20
        @test caller((a, b) -> a - b, 10, 4) == 6
    end

    @testset "Function in local variable" begin
        # Store function in variable, then pass it
        my_func = add
        @test caller(my_func, 100, 200) == 300

        my_func = mul
        @test caller(my_func, 7, 8) == 56
    end
end

true
