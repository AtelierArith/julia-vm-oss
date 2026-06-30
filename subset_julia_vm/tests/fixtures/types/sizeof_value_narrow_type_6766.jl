# Issue #6766: sizeof(x) on a value must return the logical type size,
# not the boxed Value representation size (8). It should equal
# sizeof(typeof(x)) for every bits type.

using Test

@testset "sizeof(value) matches sizeof(typeof(value)) - Issue #6766" begin
    # Signed integers
    @assert sizeof(Int8(1)) == 1
    @assert sizeof(Int16(1)) == 2
    @assert sizeof(Int32(4)) == 4
    @assert sizeof(Int64(1)) == 8
    @assert sizeof(Int128(1)) == 16

    # Unsigned integers
    @assert sizeof(UInt8(1)) == 1
    @assert sizeof(UInt16(1)) == 2
    @assert sizeof(UInt32(1)) == 4
    @assert sizeof(UInt64(1)) == 8
    @assert sizeof(UInt128(1)) == 16

    # Floating point
    @assert sizeof(Float16(1.0)) == 2
    @assert sizeof(Float32(1.0f0)) == 4
    @assert sizeof(Float64(1.0)) == 8

    # Bool and Char
    @assert sizeof(true) == 1
    @assert sizeof(false) == 1
    @assert sizeof('a') == 4

    # The value version must agree with the type version for every bits type.
    @assert sizeof(Int8(1)) == sizeof(typeof(Int8(1)))
    @assert sizeof(Int16(1)) == sizeof(typeof(Int16(1)))
    @assert sizeof(Int32(4)) == sizeof(typeof(Int32(4)))
    @assert sizeof(Int64(1)) == sizeof(typeof(Int64(1)))
    @assert sizeof(Int128(1)) == sizeof(typeof(Int128(1)))
    @assert sizeof(UInt8(1)) == sizeof(typeof(UInt8(1)))
    @assert sizeof(UInt16(1)) == sizeof(typeof(UInt16(1)))
    @assert sizeof(UInt32(1)) == sizeof(typeof(UInt32(1)))
    @assert sizeof(UInt64(1)) == sizeof(typeof(UInt64(1)))
    @assert sizeof(UInt128(1)) == sizeof(typeof(UInt128(1)))
    @assert sizeof(Float16(1.0)) == sizeof(typeof(Float16(1.0)))
    @assert sizeof(Float32(1.0f0)) == sizeof(typeof(Float32(1.0f0)))
    @assert sizeof(Float64(1.0)) == sizeof(typeof(Float64(1.0)))
    @assert sizeof(true) == sizeof(typeof(true))
    @assert sizeof('a') == sizeof(typeof('a'))

    # sizeof(::Type) must stay correct (regression guard).
    @assert sizeof(Int8) == 1
    @assert sizeof(Int16) == 2
    @assert sizeof(Int32) == 4
    @assert sizeof(Int64) == 8
    @assert sizeof(Int128) == 16
    @assert sizeof(UInt8) == 1
    @assert sizeof(UInt16) == 2
    @assert sizeof(UInt32) == 4
    @assert sizeof(UInt64) == 8
    @assert sizeof(UInt128) == 16
    @assert sizeof(Float16) == 2
    @assert sizeof(Float32) == 4
    @assert sizeof(Float64) == 8
    @assert sizeof(Bool) == 1
    @assert sizeof(Char) == 4

    @test (true)
end

true  # Test passed
