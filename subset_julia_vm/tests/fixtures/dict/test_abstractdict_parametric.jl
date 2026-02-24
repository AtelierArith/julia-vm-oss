using Test

# Define a custom mutable struct subtyping AbstractDict{K,V}
mutable struct SimpleDict{K,V} <: AbstractDict{K,V}
    count::Int64
end

@testset "AbstractDict{K,V} parametric abstract type" begin
    # Test 1: AbstractDict is recognized as an abstract type
    @test isabstracttype(AbstractDict)

    # Test 2: Mutable struct can subtype AbstractDict{K,V}
    d = SimpleDict{String, Int64}(0)
    @test isa(d, SimpleDict{String, Int64})

    # Test 3: isa check with parametric AbstractDict
    @test isa(d, AbstractDict{String, Int64})

    # Test 4: isa check with base AbstractDict (no params)
    @test isa(d, AbstractDict)

    # Test 5: Dict itself is a subtype of AbstractDict
    d2 = Dict("a" => 1)
    @test isa(d2, AbstractDict)
end

true
