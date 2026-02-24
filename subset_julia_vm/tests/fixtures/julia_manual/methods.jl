# Julia Manual: Methods (Multiple Dispatch)
# https://docs.julialang.org/en/v1/manual/methods/
# Tests multiple dispatch, method specialization, and parametric methods.

using Test

# Basic multiple dispatch
function describe(x::Int64)
    "integer: $x"
end

function describe(x::Float64)
    "float: $x"
end

function describe(x::String)
    "string: $x"
end

function describe(x::Bool)
    "bool: $x"
end

# Two-argument dispatch
function combine(x::Int64, y::Int64)
    x + y
end

function combine(x::String, y::String)
    x * y
end

function combine(x::Int64, y::String)
    string(x) * y
end

# Abstract type dispatch
abstract type Animal end

struct Dog <: Animal
    name::String
end

struct Cat <: Animal
    name::String
end

function speak(a::Dog)
    "$(a.name) says Woof!"
end

function speak(a::Cat)
    "$(a.name) says Meow!"
end

function speak(a::Animal)
    "$(a.name) makes a sound"
end

# Parametric method
function same_type(x::T, y::T) where T
    true
end

function same_type(x, y)
    false
end

@testset "Basic multiple dispatch" begin
    @test describe(42) == "integer: 42"
    @test describe(3.14) == "float: 3.14"
    @test describe("hello") == "string: hello"
    @test describe(true) == "bool: true"
end

@testset "Two-argument dispatch" begin
    @test combine(1, 2) == 3
    @test combine("hello", " world") == "hello world"
    @test combine(42, " is the answer") == "42 is the answer"
end

@testset "Abstract type dispatch" begin
    d = Dog("Rex")
    c = Cat("Whiskers")
    @test speak(d) == "Rex says Woof!"
    @test speak(c) == "Whiskers says Meow!"
end

@testset "Parametric methods" begin
    @test same_type(1, 2) == true
    @test same_type("a", "b") == true
    @test same_type(1, "a") == false
    @test same_type(1, 1.0) == false
end

true
