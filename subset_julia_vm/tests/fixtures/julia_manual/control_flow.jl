# Julia Manual: Control Flow
# https://docs.julialang.org/en/v1/manual/control-flow/
# Tests if/else, for, while, try/catch, and short-circuit evaluation.

using Test

# Helper functions defined outside testset
function fizzbuzz(n)
    if n % 15 == 0
        return "FizzBuzz"
    elseif n % 3 == 0
        return "Fizz"
    elseif n % 5 == 0
        return "Buzz"
    else
        return string(n)
    end
end

function collatz_steps(n)
    steps = 0
    while n != 1
        if n % 2 == 0
            n = div(n, 2)
        else
            n = 3n + 1
        end
        steps += 1
    end
    steps
end

function safe_sqrt(x)
    try
        if x < 0
            error("negative input")
        end
        return sqrt(Float64(x))
    catch e
        return nothing
    end
end

@testset "if-elseif-else" begin
    @test fizzbuzz(15) == "FizzBuzz"
    @test fizzbuzz(9) == "Fizz"
    @test fizzbuzz(10) == "Buzz"
    @test fizzbuzz(7) == "7"
end

@testset "Ternary operator" begin
    x = 5
    @test (x > 0 ? "positive" : "non-positive") == "positive"
    @test (x < 0 ? "negative" : "non-negative") == "non-negative"
end

@testset "Short-circuit evaluation" begin
    @test (true && true) == true
    @test (true && false) == false
    @test (false || true) == true
    @test (false || false) == false

    x = 5
    result = x > 0 && x < 10
    @test result == true
end

@testset "for loops" begin
    sum = 0
    for i in 1:5
        sum += i
    end
    @test sum == 15
end

@testset "while loops" begin
    @test collatz_steps(1) == 0
    @test collatz_steps(2) == 1
    @test collatz_steps(4) == 2
end

@testset "break and continue" begin
    # break
    result = 0
    for i in 1:100
        if i > 5
            break
        end
        result += i
    end
    @test result == 15

    # continue
    sum_odd = 0
    for i in 1:10
        if i % 2 == 0
            continue
        end
        sum_odd += i
    end
    @test sum_odd == 25
end

@testset "try-catch" begin
    @test safe_sqrt(4.0) == 2.0
    @test safe_sqrt(-1.0) === nothing
end

true
