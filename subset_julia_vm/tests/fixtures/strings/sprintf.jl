# Test sprintf function

using Test

@testset "sprintf - C-style format string" begin

    # Basic format specifiers
    result1 = sprintf("Hello %s!", "world")
    check1 = result1 == "Hello world!"

    # Integer formatting
    result2 = sprintf("x = %d, y = %i", 42, -10)
    check2 = result2 == "x = 42, y = -10"

    # Float formatting - just check it contains the expected parts
    result3 = sprintf("pi = %f", 3.14159)
    check3 = occursin("3.14159", result3)

    # Hex and octal
    result4 = sprintf("hex: %x, HEX: %X, oct: %o", 255, 255, 64)
    check4 = result4 == "hex: ff, HEX: FF, oct: 100"

    # Escaped percent
    result5 = sprintf("100%% complete")
    check5 = result5 == "100% complete"

    # Multiple args
    result6 = sprintf("%s: %d + %d = %d", "Sum", 2, 3, 5)
    check6 = result6 == "Sum: 2 + 3 = 5"

    # All checks must pass
    @test (check1 && check2 && check3 && check4 && check5 && check6)
end

true  # Test passed
