# Test sprint(f, args...) with user-defined functions
# This tests the HOF-style sprint implementation
#
# Note: Uses `print(io, x)` instead of `write(io, x)` because:
# - print(io, x) goes through emit_output which is captured by sprint
# - write(io, x) in SubsetJuliaVM returns a new IOBuffer (functional style)
#   rather than mutating the original, so it doesn't work with sprint's
#   output capture mechanism

using Test

function myprint(io, x)
    print(io, x)
end

function myprint2(io, x, y)
    print(io, x)
    print(io, y)
end

@testset "sprint(f, args...) with user-defined functions" begin

    # Define a simple print function that prints a value to IO

    # Test 1: sprint with user-defined function, single integer arg
    r1 = sprint(myprint, 42)
    check1 = length(r1) == 2  # "42" has length 2

    # Define a function that prints two args

    # Test 2: sprint with user-defined function, two integer args
    r2 = sprint(myprint2, 10, 20)
    check2 = length(r2) == 4  # "1020" has length 4

    # All checks must pass
    @test (check1 && check2)
end

true  # Test passed
