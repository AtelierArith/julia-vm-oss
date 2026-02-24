# Test sprint with varargs support
# sprint(f, args...) should handle any number of arguments

using Test

@testset "sprint with varargs support (5+ arguments)" begin

    # Test 1: sprint with 1 arg (existing behavior)
    r1 = sprint(print, 42)
    check1 = length(r1) == 2  # "42"

    # Test 2: sprint with 2 args
    r2 = sprint(print, 1, 2)
    check2 = length(r2) == 2  # "12"

    # Test 3: sprint with 3 args
    r3 = sprint(print, 1, 2, 3)
    check3 = length(r3) == 3  # "123"

    # Test 4: sprint with 5 args (previously unsupported)
    r4 = sprint(print, 1, 2, 3, 4, 5)
    check4 = length(r4) == 5  # "12345"

    # Test 5: sprint with 6 args
    r5 = sprint(print, 1, 2, 3, 4, 5, 6)
    check5 = length(r5) == 6  # "123456"

    # Test 6: sprint with mixed types
    r6 = sprint(print, "a", 1, "b", 2)
    check6 = length(r6) == 4  # "a1b2"

    # All checks must pass
    @test (check1 && check2 && check3 && check4 && check5 && check6)
end

true  # Test passed
