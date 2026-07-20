using Test

# Issues #9533/#9547: tuple-only broadcast preserves tuple result shape.

@testset "tuple broadcast materialization shape (#9533/#9547)" begin
    promoted = float.(promote(3, 4, 12))
    @test promoted === (3.0, 4.0, 12.0)
    @test typeof(promoted) === Tuple{Float64, Float64, Float64}

    literal_unary = abs.((1.0, -2.0))
    @test literal_unary === (1.0, 2.0)
    @test typeof(literal_unary) === Tuple{Float64, Float64}

    subtype_result = ((Int64, Float64, String) .<: Number)
    @test subtype_result === (true, true, false)
    @test typeof(subtype_result) === Tuple{Bool, Bool, Bool}

    @test (1, 2) .+ 10 === (11, 12)
    @test 10 .+ (1, 2) === (11, 12)
    @test (1, 2) .+ (10, 20) === (11, 22)
    @test typeof((1, 2) .+ (10, 20)) === Tuple{Int64, Int64}

    nested = abs.((1, -2) .+ (10, -20))
    @test nested === (11, 22)
    @test typeof(nested) === Tuple{Int64, Int64}

    mixed_array = (1, 2) .+ [10, 20]
    @test mixed_array == [11, 22]
    @test typeof(mixed_array) === Vector{Int64}

    empty = abs.(())
    @test empty === ()
    @test typeof(empty) === Tuple{}

    @test hypot(3, 4, 12) == 13.0
    @test typeof(hypot(3, 4, 12)) === Float64
end

true
