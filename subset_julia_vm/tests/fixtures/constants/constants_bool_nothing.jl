using Test

# Tests for boolean and nothing constants
@testset "boolean and nothing constants" begin
    # true and false are constants of type Bool
    @test true == true
    @test false == false
    @test true != false
    @test typeof(true) == Bool
    @test typeof(false) == Bool

    # Boolean arithmetic
    @test true + true == 2
    @test true + false == 1
    @test false + false == 0

    # nothing is a singleton
    @test nothing === nothing
    @test typeof(nothing) == Nothing

    # nothing in comparisons
    @test isnothing(nothing) == true
    @test isnothing(42) == false
end

true
