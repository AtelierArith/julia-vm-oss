# Test: Pairs symbol indexing for Julia compatibility
# kwargs[:key] is the correct Julia way to access kwargs

using Test

function g(; kwargs...)
    return kwargs[:a] + kwargs[:b]
end

@testset "Julia-compatible kwargs symbol indexing: kwargs[:a] + kwargs[:b]" begin


    result = g(a=10, b=20)
    @test (Float64(result)) == 30.0
end

true  # Test passed
