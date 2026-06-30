using Test

@testset "cumsum preserves upstream result element types (#4018, #4590)" begin
    ints = cumsum([1, 2, 3])
    @test ints == [1, 3, 6]
    @test typeof(ints) === Vector{Int64}
    @test eltype(ints) === Int64

    narrow = cumsum(Int8[1, 2, 3])
    @test narrow == [1, 3, 6]
    @test typeof(narrow) === Vector{Int64}
    @test eltype(narrow) === Int64

    unsigned = cumsum(UInt8[1, 2, 3])
    @test unsigned[1] == UInt64(1)
    @test unsigned[2] == UInt64(3)
    @test unsigned[3] == UInt64(6)
    @test typeof(unsigned) === Vector{UInt64}
    @test eltype(unsigned) === UInt64

    floats32 = cumsum(Float32[1, 2, 3])
    @test floats32[1] === Float32(1)
    @test floats32[2] === Float32(3)
    @test floats32[3] === Float32(6)
    @test typeof(floats32) === Vector{Float32}
    @test eltype(floats32) === Float32

    bools = cumsum(Bool[true, false, true])
    @test bools == [1, 1, 2]
    @test typeof(bools) === Vector{Int64}
    @test eltype(bools) === Int64
end

@testset "cumprod preserves upstream result element types (#4018, #4590)" begin
    ints = cumprod([1, 2, 3])
    @test ints == [1, 2, 6]
    @test typeof(ints) === Vector{Int64}
    @test eltype(ints) === Int64

    narrow = cumprod(Int16[1, 2, 3])
    @test narrow == [1, 2, 6]
    @test typeof(narrow) === Vector{Int64}
    @test eltype(narrow) === Int64

    unsigned = cumprod(UInt16[1, 2, 3])
    @test unsigned[1] == UInt64(1)
    @test unsigned[2] == UInt64(2)
    @test unsigned[3] == UInt64(6)
    @test typeof(unsigned) === Vector{UInt64}
    @test eltype(unsigned) === UInt64

    floats32 = cumprod(Float32[1, 2, 3])
    @test floats32[1] === Float32(1)
    @test floats32[2] === Float32(2)
    @test floats32[3] === Float32(6)
    @test typeof(floats32) === Vector{Float32}
    @test eltype(floats32) === Float32

    bools = cumprod(Bool[true, false, true])
    @test bools == Bool[true, false, false]
    @test typeof(bools) === Vector{Bool}
    @test eltype(bools) === Bool
end

true
