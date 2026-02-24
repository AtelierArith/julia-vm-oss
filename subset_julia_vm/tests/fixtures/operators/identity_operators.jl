# Test !== (≢) operator - not identical
# !==(a, b) should be equivalent to !(a === b)

using Test

@testset "!== (≢) operator - not identical (negation of ===)" begin

    result = 0

    # Test !== with different values
    if 1 !== 2
        result = result + 1
    end

    # Test !== with same values (should be false)
    if !(1 !== 1)
        result = result + 1
    end

    # Test !== with NaN (NaN === NaN is true in Julia, so NaN !== NaN is false)
    x = 0.0 / 0.0  # NaN
    if !(x !== x)  # NaN === NaN is true, so NaN !== NaN is false
        result = result + 1
    end

    # Test !== with -0.0 and 0.0 (-0.0 === 0.0 is false in Julia)
    if -0.0 !== 0.0
        result = result + 1
    end

    @test (result) == 4
end

true  # Test passed
