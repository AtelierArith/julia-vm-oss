# Julia Manual: Scope of Variables
# https://docs.julialang.org/en/v1/manual/variables-and-scoping/
# Tests local/global scope, let blocks, and closures.

using Test

# Global scope variable
global_var = 100

function read_global()
    global_var
end

function modify_with_local()
    x = 42
    x
end

function make_counter()
    count = 0
    function increment()
        count += 1
        count
    end
    increment
end

function scope_outer(x)
    function scope_inner(y)
        x + y
    end
    scope_inner(10)
end

@testset "Global scope" begin
    @test global_var == 100
    @test read_global() == 100
end

@testset "Local scope in functions" begin
    @test modify_with_local() == 42
end

@testset "Let blocks" begin
    result = let
        x = 10
        y = 20
        x + y
    end
    @test result == 30
end

@testset "For loop scope" begin
    total = 0
    for i in 1:5
        total += i
    end
    @test total == 15
end

@testset "Closures capture scope" begin
    counter = make_counter()
    @test counter() == 1
    @test counter() == 2
    @test counter() == 3
end

@testset "Nested function scope" begin
    @test scope_outer(5) == 15
end

true
