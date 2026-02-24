# Test hasmethod function
# hasmethod(f, types) returns true if a method exists for the given signature

using Test

function foo(x::Int64)
    x + 1
end

function foo(x::Float64)
    x * 2.0
end

function foo(x::Int64, y::Int64)
    x + y
end

@testset "hasmethod - check if method exists for given signature" begin




    # Should return true for matching signatures
    r1 = hasmethod(foo, Tuple{Int64})
    r2 = hasmethod(foo, Tuple{Float64})
    r3 = hasmethod(foo, Tuple{Int64, Int64})

    # Should return false for non-matching signatures
    r4 = hasmethod(foo, Tuple{String})
    r5 = hasmethod(foo, Tuple{Int64, Float64})

    # Return final result
    @test (r1 && r2 && r3 && !r4 && !r5)
end

true  # Test passed
