# Test type variance: Tuple is covariant, Array is invariant

using Test

@testset "Type variance" begin
    # Test 1: Tuple is COVARIANT
    # Tuple{Int64} <: Tuple{Number} because Int64 <: Number
    @test Tuple{Int64} <: Tuple{Number}
    @test Tuple{Int64} <: Tuple{Real}
    @test Tuple{Int64} <: Tuple{Integer}
    @test Tuple{Float64} <: Tuple{Real}
    @test Tuple{Float64} <: Tuple{Number}

    # Test 2: Tuple covariance with multiple elements
    @test Tuple{Int64, Float64} <: Tuple{Number, Number}
    @test Tuple{Int64, Int64} <: Tuple{Integer, Integer}
    @test Tuple{Int64, String} <: Tuple{Number, AbstractString}

    # Test 3: Array is INVARIANT
    # Vector{Int64} is NOT a subtype of Vector{Number}
    @test !(Vector{Int64} <: Vector{Number})
    @test !(Vector{Float64} <: Vector{Real})
    @test !(Array{Int64} <: Array{Number})

    # Test 4: Array variance - only exact matches are subtypes
    @test Vector{Int64} <: Vector{Int64}
    @test Vector{Float64} <: Vector{Float64}

    # Test 5: Tuple variance - reflexivity
    @test Tuple{Int64} <: Tuple{Int64}
    @test Tuple{String} <: Tuple{String}
end

true
