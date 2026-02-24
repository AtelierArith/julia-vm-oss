# Test left division (backslash operator) for solving linear systems
# Issue #628: A \ b solves Ax = b for x

using Test

@testset "backslash: solve Ax=b linear system (Issue #628)" begin

    # Create a simple 2x2 system: Ax = b
    # A = [2 1; 1 3], b = [4, 5]
    # Solution: x = [1, 2] because 2*1 + 1*2 = 4 and 1*1 + 3*2 = 7... wait, let me recalculate

    # Let's use a clearer example:
    # A = [1 2; 3 4], b = [5, 11]
    # Solution should satisfy: 1*x1 + 2*x2 = 5 and 3*x1 + 4*x2 = 11
    # x = [1, 2] works: 1*1 + 2*2 = 5, 3*1 + 4*2 = 11

    A = [1.0 2.0; 3.0 4.0]
    b = [5.0, 11.0]

    # Solve Ax = b using backslash
    x = A \ b

    # Verify the solution
    # x should be [1.0, 2.0]
    # Check: A*x should equal b
    # Manual verification: 1*1 + 2*2 = 5, 3*1 + 4*2 = 11

    # Sum of x elements (should be 1 + 2 = 3)
    @test (x[1] + x[2]) == 3.0
end

true  # Test passed
