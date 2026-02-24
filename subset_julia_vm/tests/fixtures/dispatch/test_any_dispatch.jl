# Test runtime dispatch for Any-typed parameters

using Test

function process(x::Int64)
    return 1
end

function process(x::String)
    return 2
end

function process(x::Float64)
    return 3
end

function call_process(x)
    process(x)
end

@testset "Runtime dispatch for Any-typed parameters (Issue #401)" begin

    # Define specialized methods



    # Wrapper function with Any-typed parameter

    # Test dispatch through Any-typed wrapper
    r1 = call_process(42)
    check1 = r1 == 1

    r2 = call_process("hello")
    check2 = r2 == 2

    r3 = call_process(3.14)
    check3 = r3 == 3

    # Test with array of mixed types
    arr = [42, "hello", 3.14]
    results = Int64[]
    for item in arr
        push!(results, call_process(item))
    end
    check4 = results[1] == 1 && results[2] == 2 && results[3] == 3

    # All checks must pass
    @test (check1 && check2 && check3 && check4)
end

true  # Test passed
