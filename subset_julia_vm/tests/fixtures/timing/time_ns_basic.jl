# Test time_ns() builtin function
# Returns current time in nanoseconds as Int64

using Test

@testset "time_ns() returns current time in nanoseconds" begin

    # Get current time
    t1 = time_ns()

    # Do enough work to ensure measurable time passes
    x = 0.0
    for i in 1:10000
        x = x + Float64(i)
    end

    # Get time again
    t2 = time_ns()

    # t2 should be >= t1 and t1 should be positive
    # Use >= instead of > for robustness on fast systems with low clock resolution
    @test (t2 >= t1 && t1 > 0)
end

true  # Test passed
