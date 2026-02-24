# Test BigInt predicates (Issue #416)
# Based on Julia's base/gmp.jl

using Test

@testset "BigInt predicates: iszero, isone, sign (Issue #416)" begin

    result = 0.0

    # Test iszero(::BigInt)
    if iszero(big(0)) == true
        result = result + 1.0
    end
    if iszero(big(1)) == false
        result = result + 1.0
    end
    if iszero(big(-1)) == false
        result = result + 1.0
    end

    # Test isone(::BigInt)
    if isone(big(1)) == true
        result = result + 1.0
    end
    if isone(big(0)) == false
        result = result + 1.0
    end
    if isone(big(2)) == false
        result = result + 1.0
    end

    # Test sign(::BigInt)
    if sign(big(-5)) == -1
        result = result + 1.0
    end
    if sign(big(0)) == 0
        result = result + 1.0
    end
    if sign(big(5)) == 1
        result = result + 1.0
    end

    @test (result) == 9.0
end

true  # Test passed
