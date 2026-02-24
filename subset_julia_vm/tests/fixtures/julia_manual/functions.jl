# Julia Manual: Functions
# https://docs.julialang.org/en/v1/manual/functions/
# Tests function definitions, anonymous functions, varargs, and do blocks.

using Test

# Full form function definition
function add(x, y)
    x + y
end

# Short form function definition
multiply(x, y) = x * y

# Function with explicit return
function safe_div(x, y)
    if y == 0
        return nothing
    end
    return x / y
end

# Multiple return values
function swap(a, b)
    return (b, a)
end

# Default arguments
function greet(name)
    "Hello, $(name)!"
end

function greet(name, greeting)
    "$(greeting), $(name)!"
end

# Keyword arguments
function circle_area(; radius=1.0)
    pi * radius^2
end

# Varargs
function mysum(args...)
    s = 0
    for a in args
        s += a
    end
    s
end

# Function with type annotation
function typed_add(x::Int64, y::Int64)
    x + y
end

@testset "Function definitions" begin
    @test add(2, 3) == 5
    @test multiply(3, 4) == 12
    @test safe_div(10, 2) == 5.0
    @test safe_div(10, 0) === nothing
end

@testset "Multiple return values" begin
    a, b = swap(1, 2)
    @test a == 2
    @test b == 1
end

@testset "Default arguments" begin
    @test greet("Julia") == "Hello, Julia!"
    @test greet("Julia", "Hi") == "Hi, Julia!"
end

@testset "Keyword arguments" begin
    @test circle_area() == pi
    @test circle_area(radius=2.0) == 4.0 * pi
end

@testset "Varargs functions" begin
    @test mysum(1, 2, 3) == 6
    @test mysum(1) == 1
    @test mysum(1, 2, 3, 4, 5) == 15
end

@testset "Anonymous functions" begin
    f = x -> x^2
    @test f(3) == 9
    @test f(5) == 25

    g = (x, y) -> x + y
    @test g(2, 3) == 5
end

@testset "Function composition and piping" begin
    double = x -> 2x
    square = x -> x^2

    @test (square(double(3))) == 36
    @test (3 |> double |> square) == 36
end

@testset "Do block syntax" begin
    result = map([1, 2, 3]) do x
        x^2
    end
    @test result == [1, 4, 9]

    filtered = filter([1, 2, 3, 4, 5]) do x
        x > 2
    end
    @test filtered == [3, 4, 5]
end

@testset "Typed function dispatch" begin
    @test typed_add(2, 3) == 5
end

true
