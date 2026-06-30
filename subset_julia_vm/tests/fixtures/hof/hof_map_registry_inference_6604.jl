# map call-site lambda return-type inference, after migrating the `map` rule
# onto the tfuncs registry path via the HofLambdaAnalyzer seam (Issue #6604).
#
# The registry rule `tfuncs::hof_ops::map_call_result` receives the
# function-argument expression through `TFuncContext::arg_exprs` and calls back
# into `CoreCompiler` (as a `HofLambdaAnalyzer`) to infer the lambda's mapped
# element type. These `typeof` assertions pin that the migrated path produces
# the same concrete `Vector{T}` element types as before.

using Test

@testset "hof_map_registry_inference_6604: map call-site lambda inference" begin
    # Inline lambda whose body returns Float64 -> Vector{Float64}
    r1 = map(x -> x * 2.0, [1, 2, 3])
    @test r1 == [2.0, 4.0, 6.0]
    @test typeof(r1) === Vector{Float64}

    # Inline lambda preserving Int -> Vector{Int64}
    r2 = map(x -> x + 1, [1, 2, 3])
    @test r2 == [2, 3, 4]
    @test typeof(r2) === Vector{Int64}

    # Named type-converter callable -> Vector{Float64}
    r3 = map(Float64, [1, 2, 3])
    @test r3 == [1.0, 2.0, 3.0]
    @test typeof(r3) === Vector{Float64}

    # Named function (abs) preserves Int element type
    r4 = map(abs, [-1, -2, 3])
    @test r4 == [1, 2, 3]
    @test typeof(r4) === Vector{Int64}

    # Predicate lambda returning Bool -> Vector{Bool}
    r5 = map(x -> x > 0, [-1, 0, 1])
    @test r5 == [false, false, true]
    @test typeof(r5) === Vector{Bool}

    # map over a Float64 array with a float lambda
    r6 = map(x -> x + 0.5, [1.0, 2.0, 3.0])
    @test r6 == [1.5, 2.5, 3.5]
    @test typeof(r6) === Vector{Float64}

    # Qualified Base.map uses the same migrated inference path
    r7 = Base.map(x -> x * 2.0, [1, 2, 3])
    @test r7 == [2.0, 4.0, 6.0]
    @test typeof(r7) === Vector{Float64}
end

true
