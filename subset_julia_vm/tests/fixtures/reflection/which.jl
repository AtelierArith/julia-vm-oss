# Test which function
# which(f, types) returns the Method that would be called for the given signature

using Test

function bar(x::Int64)
    x + 1
end

function bar(x::Float64)
    x * 2.0
end

@testset "which - get the method that would be called" begin



    # Get the method for Int64 signature.
    # Method.nargs includes the function object and is Int32 (Issue #4989),
    # so a one-explicit-argument method reports nargs == 2.
    m1 = which(bar, Tuple{Int64})
    check1 = m1.name == :bar
    check2 = m1.nargs == 2
    check2b = m1.nargs isa Int32

    # Get the method for Float64 signature
    m2 = which(bar, Tuple{Float64})
    check3 = m2.name == :bar
    check4 = m2.nargs == 2

    # Return final result
    @test (check1 && check2 && check2b && check3 && check4)
end

true  # Test passed
