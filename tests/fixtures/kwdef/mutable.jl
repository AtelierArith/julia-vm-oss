# Test @kwdef with mutable structs

using Test

@kwdef mutable struct Config
    debug::Bool = false
    value::Int64 = 42
end

@testset "@kwdef with mutable struct" begin
    # Default constructor
    c1 = Config()
    @test c1.debug == false
    @test c1.value == 42

    # Partial kwargs
    c2 = Config(debug=true)
    @test c2.debug == true
    @test c2.value == 42

    # All kwargs
    c3 = Config(debug=true, value=100)
    @test c3.debug == true
    @test c3.value == 100

    # Test mutability
    c4 = Config()
    c4.debug = true
    c4.value = 99
    @test c4.debug == true
    @test c4.value == 99
end

true
