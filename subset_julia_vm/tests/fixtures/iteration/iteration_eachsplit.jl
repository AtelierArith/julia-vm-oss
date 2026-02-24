# Test eachsplit iterator - split string by delimiter

using Test

@testset "eachsplit - split string into iterator" begin

    # Basic split by comma
    es = eachsplit("abc,def,ghi", ",")

    # First element
    next = iterate(es)
    @assert next !== nothing
    @assert length(next[1]) == 3

    # Second element
    next = iterate(es, next[2])
    @assert next !== nothing
    @assert length(next[1]) == 3

    # Third element
    next = iterate(es, next[2])
    @assert next !== nothing
    @assert length(next[1]) == 3

    # No more elements
    next = iterate(es, next[2])
    @assert next === nothing

    @test (true)
end

true  # Test passed
