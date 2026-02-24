using Test

# Abstract types with parametric subtypes
abstract type Container{T} end

struct Box{T} <: Container{T}
    value::T
end

function unwrap(c::Container{T}) where T
    c.value
end

function same_type(a::Container{T}, b::Container{T}) where T
    true
end

@testset "abstract types with parametric structs" begin
    int_box = Box{Int64}(42)
    str_box = Box{String}("hello")

    @test unwrap(int_box) == 42
    @test unwrap(str_box) == "hello"

    @test Box{Int64} <: Container{Int64}
    @test Box{String} <: Container{String}

    @test same_type(Box{Int64}(1), Box{Int64}(2)) == true
end

true
