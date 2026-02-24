# Test pop! function for Dict
# pop!(dict, key) - remove and return value for key, error if not found
# pop!(dict, key, default) - remove and return value, or default if not found

using Test

@testset "pop! for Dict - remove and return value for key" begin

    result = true

    # Test pop! with existing key (integer values)
    d1 = Dict(1 => 100, 2 => 200, 3 => 300)
    v1 = pop!(d1, 2, 0)
    result = result && v1 == 200
    result = result && !haskey(d1, 2)
    result = result && haskey(d1, 1)
    result = result && haskey(d1, 3)

    # Test pop! with default - key exists
    d2 = Dict(10 => 100, 20 => 200)
    v2 = pop!(d2, 10, -1)
    result = result && v2 == 100
    result = result && !haskey(d2, 10)

    # Test pop! with default - key doesn't exist
    d3 = Dict(10 => 100, 20 => 200)
    v3 = pop!(d3, 99, -1)
    result = result && v3 == -1
    result = result && haskey(d3, 10)  # other keys should remain
    result = result && haskey(d3, 20)

    # Test multiple pops
    d4 = Dict(1 => 10, 2 => 20, 3 => 30)
    v4a = pop!(d4, 1, 0)
    v4b = pop!(d4, 2, 0)
    v4c = pop!(d4, 3, 0)
    result = result && v4a == 10
    result = result && v4b == 20
    result = result && v4c == 30
    result = result && length(d4) == 0

    @test (result)
end

true  # Test passed
