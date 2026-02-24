# Test sizeof function - get size of value in bytes

using Test

@testset "sizeof - get size of value in bytes" begin

    # Primitive types
    @assert sizeof(1) == 8          # Int64 is 8 bytes
    @assert sizeof(1.0) == 8        # Float64 is 8 bytes
    @assert sizeof(true) == 1       # Bool is 1 byte
    @assert sizeof('a') == 4        # Char is 4 bytes (Unicode)

    # String size is number of bytes
    @assert sizeof("hello") == 5
    @assert sizeof("") == 0

    # Array size is element_size * num_elements
    arr = [1.0, 2.0, 3.0]
    @assert sizeof(arr) == 24  # 3 elements * 8 bytes

    # Nothing has size 0
    @assert sizeof(nothing) == 0

    @test (true)
end

true  # Test passed
