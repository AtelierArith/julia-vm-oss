# Test @kwdef with mutable struct

using Test

@kwdef mutable struct Config
    debug::Bool = false
    value::Int64 = 42
end

@testset "@kwdef with mutable struct" begin
    c = Config(debug=true)
    @test c.value == 42

    # Test mutability
    c.value = 100
    @test c.value == 100
end

true
