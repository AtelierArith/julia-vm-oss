# Test first and last functions with arrays

using Test

@testset "first and last functions work with arrays" begin

    # Integer arrays
    a = [10, 20, 30, 40, 50]
    r1 = first(a) == 10
    r2 = last(a) == 50

    # Float arrays
    b = [1.5, 2.5, 3.5]
    r3 = first(b) == 1.5
    r4 = last(b) == 3.5

    # Single element array
    c = [42]
    r5 = first(c) == 42
    r6 = last(c) == 42

    # Tuples still work
    t = (100, 200, 300)
    r7 = first(t) == 100
    r8 = last(t) == 300

    @test ((r1 && r2 && r3 && r4 && r5 && r6 && r7 && r8) ? 1 : 0) == 1.0
end

true  # Test passed
