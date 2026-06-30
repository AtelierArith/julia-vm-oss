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

    int8s = Vector{Int8}(undef, 4)
    int8s[1] = Int8(1)
    int8s[2] = Int8(2)
    int8s[3] = Int8(3)
    int8s[4] = Int8(4)
    @assert typeof(int8s) === Vector{Int8}
    @assert eltype(int8s) === Int8
    @assert sizeof(int8s) == 4

    int16s = Vector{Int16}(undef, 3)
    int16s[1] = Int16(1)
    int16s[2] = Int16(2)
    int16s[3] = Int16(3)
    @assert typeof(int16s) === Vector{Int16}
    @assert eltype(int16s) === Int16
    @assert sizeof(int16s) == 6

    int32s = Vector{Int32}(undef, 2)
    int32s[1] = Int32(1)
    int32s[2] = Int32(2)
    @assert typeof(int32s) === Vector{Int32}
    @assert eltype(int32s) === Int32
    @assert sizeof(int32s) == 8

    uint8s = Vector{UInt8}(undef, 4)
    uint8s[1] = UInt8(1)
    uint8s[2] = UInt8(2)
    uint8s[3] = UInt8(3)
    uint8s[4] = UInt8(4)
    @assert typeof(uint8s) === Vector{UInt8}
    @assert eltype(uint8s) === UInt8
    @assert sizeof(uint8s) == 4

    uint16s = Vector{UInt16}(undef, 3)
    uint16s[1] = UInt16(1)
    uint16s[2] = UInt16(2)
    uint16s[3] = UInt16(3)
    @assert typeof(uint16s) === Vector{UInt16}
    @assert eltype(uint16s) === UInt16
    @assert sizeof(uint16s) == 6

    float32s = Vector{Float32}(undef, 2)
    float32s[1] = Float32(1.0)
    float32s[2] = Float32(2.0)
    @assert sizeof(float32s) == 8

    bools = Bool[]
    push!(bools, true)
    push!(bools, false)
    push!(bools, true)
    @assert sizeof(bools) == 3

    chars = Char[]
    push!(chars, 'a')
    push!(chars, 'b')
    @assert sizeof(chars) == 8

    # Nothing has size 0
    @assert sizeof(nothing) == 0
    @assert sizeof(missing) == 0

    @test (true)
end

true  # Test passed
