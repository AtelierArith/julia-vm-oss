# Test identity function
# identity(x) returns x unchanged

using Test

@testset "identity function - returns argument unchanged" begin

    # Test with integer
    x1 = 42
    check1 = identity(x1) == 42

    # Test with float
    x2 = 3.14
    check2 = identity(x2) == 3.14

    # Test with array
    x3 = [1, 2, 3]
    check3 = identity(x3) == [1, 2, 3]

    # Test with tuple
    x4 = (1, 2, 3)
    check4 = identity(x4) == (1, 2, 3)

    # Test identity preserves object identity (===)
    x5 = [1, 2, 3]
    check5 = identity(x5) === x5

    # All checks must pass
    @test (check1 && check2 && check3 && check4 && check5)
end

true  # Test passed
