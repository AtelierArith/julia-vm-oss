using Test

mutable struct Box{T}
    x::T
end

@testset "Parametric mutable struct" begin
    # Basic construction with explicit type parameter
    b = Box{Int64}(42)
    @test typeof(b) == Box{Int64}
    @test b.x == 42

    # Field mutation
    b.x = 100
    @test b.x == 100

    # Type inference from arguments
    b2 = Box(3.14)
    @test typeof(b2) == Box{Float64}
    @test b2.x == 3.14

    # Mutation with different value
    b2.x = 2.71
    @test b2.x == 2.71
end

true
