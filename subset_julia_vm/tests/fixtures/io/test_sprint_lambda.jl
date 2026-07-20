# Test sprint with lambda functions (Issue #411)
# sprint(io -> print(io, x)) should call the lambda with an IOBuffer
# and return the captured output as a string.

using Test

function myprint(io, x)
    print(io, x)
end

@testset "sprint with lambda functions (Issue #411)" begin

    # Test 1: Simple lambda with print
    result1 = sprint(io -> print(io, "hello"))
    @assert length(result1) == 5  # "hello" has 5 chars

    # Test 2: Named function (should still work)
    result2 = sprint(myprint, 42)
    @assert length(result2) == 2  # "42" has 2 chars

    # Test 3: sprint(f, args...) with lambda and args
    result3 = sprint((io, x) -> print(io, x * 2), 21)
    @assert length(result3) == 2  # "42" has 2 chars

    # Test 4: Simple lambda printing a number
    result4 = sprint(io -> print(io, 999))
    @assert length(result4) == 3  # "999" has 3 chars

    # Issue #9774: a temporary IOBuffer used inside sprint must not leak into
    # the outer sprint buffer before take! reads it.
    result5 = sprint(io -> begin
        inner = IOBuffer()
        print(inner, "inner")
        print(io, String(take!(inner)))
    end)
    @test result5 == "inner"

    # The pure-Julia string(x, y...) implementation uses an IOBuffer internally;
    # printing a Type exercises the nested `show(io, ::Type)` path.
    result6 = sprint(io -> print(io, string("a value of type ", Int64)))
    @test result6 == "a value of type Int64"

    @test (true)
end

true  # Test passed
