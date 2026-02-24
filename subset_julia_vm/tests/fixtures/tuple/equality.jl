# Tuple equality comparison tests

using Test

function test_tuple_equality()
    a = (1, 2)
    b = (1, 2)
    c = (1, 3)

    # Test == operator
    if !(a == b)  # should be true
        return 1
    end
    if a == c  # should be false
        return 2
    end
    if !((1, 2) == (1, 2))  # literal comparison, should be true
        return 3
    end

    # Test != operator
    if !(a != c)  # should be true
        return 4
    end
    if a != b  # should be false
        return 5
    end
    if !((1, 2) != (1, 3))  # literal comparison, should be true
        return 6
    end

    # Test in if statement
    if (1, 2) == (1, 2)
        # passed
    else
        return 7
    end

    if (1, 2) != (1, 3)
        # passed
    else
        return 8
    end

    # All tests passed
    100.0
end

@testset "Tuple == and != comparison operators" begin


    @test (test_tuple_equality()) == 100.0
end

true  # Test passed
