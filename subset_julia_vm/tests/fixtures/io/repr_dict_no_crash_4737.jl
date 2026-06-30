using Test

@testset "repr(Dict) does not crash on CartesianIndex dispatch (Issue #4737)" begin
    # Single-entry Dicts so we can assert exact output without
    # depending on hash iteration order.
    @test repr(Dict("a" => 1)) == "Dict(\"a\" => 1)"
    @test repr(Dict("hello" => 42)) == "Dict(\"hello\" => 42)"
end

@testset "repr(Dict) Symbol keys, mixed value types (Issue #4737)" begin
    @test repr(Dict(:x => 1)) == "Dict(:x => 1)"
    @test repr(Dict(1 => "one")) == "Dict(1 => \"one\")"
    @test repr(Dict(:k => "value")) == "Dict(:k => \"value\")"
end

@testset "repr(Dict) agrees with string(Dict) for the same input (Issue #4737)" begin
    # The show(io, ::AbstractDict) method added in this PR makes
    # repr's IOBuffer+show+take! path produce the same output as
    # format_dict_value uses for string() — they must agree.
    d1 = Dict("a" => 1)
    @test repr(d1) == string(d1)
    d2 = Dict(:x => "v")
    @test repr(d2) == string(d2)
end

true
