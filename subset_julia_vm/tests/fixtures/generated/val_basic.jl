# Test: Basic Val{N} type usage
# Phase 2: Verify Val type can be created and passed to functions

using Test

function val_test(::Val{1})
    10
end

function val_test(::Val{2})
    20
end

@testset "Val{N} type basic usage and function dispatch" begin

    # Define two methods with distinct Val types


    # Call with Val{1}
    r1 = val_test(Val{1}())
    println("r1 = ", r1)
    @assert r1 == 10

    # Call with Val{2}
    r2 = val_test(Val{2}())
    println("r2 = ", r2)
    @assert r2 == 20

    # Return true if all tests passed
    @test (true)
end

true  # Test passed
