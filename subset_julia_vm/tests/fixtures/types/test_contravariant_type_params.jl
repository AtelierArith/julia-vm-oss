# Test contravariant type parameters (Issue #465)
# Contravariant syntax {>:T} represents a set of types (UnionAll)

using Test

# Type definitions must be outside @testset
abstract type Shape{T} end
struct Circle{T} <: Shape{T}
    radius::T
end

@testset "Contravariant type parameters" begin
    # Test 1: Contravariant type relationships
    # Shape{>:Int64} is a UnionAll type representing all Shape{T} where T >: Int64
    @test Circle{Real} <: Shape{>:Int64}
    @test Circle{Number} <: Shape{>:Int64}
    @test Circle{Any} <: Shape{>:Int64}
    @test Circle{Int64} <: Shape{>:Int64}

    # Test 2: Contravariant does not match subtypes
    @test !(Circle{Int32} <: Shape{>:Int64})

    # Test 3: Array contravariant types
    # Array{>:Int64} represents all Array{T} where T >: Int64
    @test Array{Real} <: Array{>:Int64}
    @test Array{Number} <: Array{>:Int64}
    @test Array{Any} <: Array{>:Int64}
    @test Array{Int64} <: Array{>:Int64}

    # Test 4: Vector contravariant types
    @test Vector{Real} <: Vector{>:Integer}
    @test Vector{Number} <: Vector{>:Integer}
    @test !(Vector{Int64} <: Vector{>:Integer})  # Int64 is not a supertype of Integer
end

true
