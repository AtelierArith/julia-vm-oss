# Test Meta.unescape function

using Test

@testset "Meta.unescape - peel away escape expressions" begin

    # Test 1: Non-escape expression returns unchanged (check head and structure)
    ex1 = :(1 + 2)
    result1 = Meta.unescape(ex1)
    @assert result1.head === :call "Test 1 failed"

    # Test 2: Escape expression gets unwrapped
    ex2 = Expr(:escape, :(x + 1))
    result2 = Meta.unescape(ex2)
    @assert result2.head === :call "Test 2 failed"

    # Test 3: Nested escape expressions get fully unwrapped
    ex3 = Expr(:escape, Expr(:escape, :(y * 2)))
    result3 = Meta.unescape(ex3)
    @assert result3.head === :call "Test 3 failed"

    # Test 4: Escape inside block gets unwrapped
    ex4 = Expr(:block, Expr(:escape, :(z - 3)))
    result4 = Meta.unescape(ex4)
    @assert result4.head === :call "Test 4 failed"

    # Test 5: Symbol returns unchanged
    @assert Meta.unescape(:x) === :x "Test 5 failed"

    # Test 6: Integer returns unchanged
    @assert Meta.unescape(123) === 123 "Test 6 failed"

    # Return success
    @test (true)
end

true  # Test passed
