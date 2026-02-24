# Test big(::Type{T}) - type to type conversions

using Test

@testset "big(::Type{T}) type-to-type conversions" begin

    # Integer types → BigInt
    result = 0

    # Test big(Int64) === BigInt
    if big(Int64) === BigInt
        result += 1
    end

    # Test big(Int32) === BigInt
    if big(Int32) === BigInt
        result += 1
    end

    # Test big(Int16) === BigInt
    if big(Int16) === BigInt
        result += 1
    end

    # Test big(Int8) === BigInt
    if big(Int8) === BigInt
        result += 1
    end

    # Test big(UInt64) === BigInt
    if big(UInt64) === BigInt
        result += 1
    end

    # Float types → BigFloat
    # Test big(Float64) === BigFloat
    if big(Float64) === BigFloat
        result += 1
    end

    # Test big(Float32) === BigFloat
    if big(Float32) === BigFloat
        result += 1
    end

    # Identity types
    # Test big(BigInt) === BigInt
    if big(BigInt) === BigInt
        result += 1
    end

    # Test big(BigFloat) === BigFloat
    if big(BigFloat) === BigFloat
        result += 1
    end

    # Return count of passed tests (should be 9)
    @test (result) == 9
end

true  # Test passed
