# Test Meta.unblock function

using Test

@testset "Meta.unblock - peel away redundant block expressions" begin

    # Test 1: Non-block expression returns unchanged (check head and structure)
    ex1 = :(1 + 2)
    result1 = Meta.unblock(ex1)
    @assert result1.head === :call "Test 1 failed"

    # Test 2: Block with single expression gets unwrapped
    ex2 = Expr(:block, :(x + 1))
    result2 = Meta.unblock(ex2)
    @assert result2.head === :call "Test 2 failed"

    # Test 3: Block with multiple expressions stays as block
    ex3 = Expr(:block, :(x = 1), :(y = 2))
    result3 = Meta.unblock(ex3)
    @assert result3.head === :block "Test 3 failed"

    # Test 4: Nested blocks get fully unwrapped
    ex4 = Expr(:block, Expr(:block, :(a + b)))
    result4 = Meta.unblock(ex4)
    @assert result4.head === :call "Test 4 failed"

    # Test 5: Symbol returns unchanged
    s = :x
    @assert Meta.unblock(s) === :x "Test 5 failed"

    # Test 6: Integer returns unchanged
    @assert Meta.unblock(42) === 42 "Test 6 failed"

    # Test 7: Block with LineNumberNode and single expression gets unwrapped
    ex7 = Expr(:block, LineNumberNode(1), :(z * 2))
    result7 = Meta.unblock(ex7)
    @assert result7.head === :call "Test 7 failed"

    # Return success
    @test (true)
end

true  # Test passed
