using Test

@testset "Dict type parameters" begin
    # Typed Dict constructor
    d1 = Dict{String, Int64}()
    @test typeof(d1) == Dict{String, Int64}

    # Typed Dict with initial values
    d2 = Dict{String, Int64}("a" => 1, "b" => 2)
    @test typeof(d2) == Dict{String, Int64}

    # Untyped Dict (should remain Dict{Any, Any})
    d3 = Dict()
    @test typeof(d3) == Dict{Any, Any}

    # Untyped Dict with values infers key/value types from pair arguments.
    d4 = Dict("x" => 10)
    @test typeof(d4) == Dict{String, Int64}
end

true
