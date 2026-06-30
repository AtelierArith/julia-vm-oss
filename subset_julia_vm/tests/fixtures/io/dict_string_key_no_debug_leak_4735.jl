using Test

@testset "string(Dict) renders String keys quoted, not Rust Debug (Issue #4735)" begin
    # Single-entry Dicts so we can assert exact string output without
    # depending on hash iteration order.
    @test string(Dict("a" => 1)) == "Dict(\"a\" => 1)"
    @test string(Dict("hello" => 42)) == "Dict(\"hello\" => 42)"
    # NOTE: Dict{T1,T2} type-parameter prefix and Bool→1 conversion
    # differ between sjulia ("Dict(\"\" => true)") and upstream
    # ("Dict{String, Bool}(\"\" => 1)"). Not part of #4735.
end

@testset "string(Dict) renders Symbol keys with ':' prefix (Issue #4735)" begin
    @test string(Dict(:x => 1)) == "Dict(:x => 1)"
    @test string(Dict(:foo => "bar")) == "Dict(:foo => \"bar\")"
end

@testset "string(Dict) renders String values quoted (Issue #4735)" begin
    @test string(Dict(1 => "one")) == "Dict(1 => \"one\")"
    @test string(Dict(:k => "value")) == "Dict(:k => \"value\")"
end

@testset "string(Dict) keeps Int/UInt keys readable (Issue #4735)" begin
    @test string(Dict(42 => "x")) == "Dict(42 => \"x\")"
    @test string(Dict(-7 => "neg")) == "Dict(-7 => \"neg\")"
end

true
