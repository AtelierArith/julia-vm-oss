# Test collect type inference
# collect should return Int64 array for integer ranges

using Test

@testset "collect returns typed arrays (Int64 for integer ranges)" begin

    # Integer range -> Int64 array
    x = collect(1:5)
    @assert eltype(x) === Int64
    @assert x == [1, 2, 3, 4, 5]

    # Float range -> Float64 array
    y = collect(1.0:0.5:3.0)
    @assert eltype(y) === Float64
    @assert length(y) == 5

    # Step range with integers -> Int64 array
    z = collect(1:2:9)
    @assert eltype(z) === Int64
    @assert z == [1, 3, 5, 7, 9]

    # Negative step range
    w = collect(5:-1:1)
    @assert eltype(w) === Int64
    @assert w == [5, 4, 3, 2, 1]

    # Return true to indicate success
    @test (true)
end

true  # Test passed
