# Test LineNumberNode insertion in quote blocks
# Quote blocks should insert LineNumberNode before each statement

using Test

@testset "LineNumberNode insertion in quote blocks" begin

    ex = quote
        x = 1
        y = 2
    end

    # The block should contain LineNumberNodes
    # Check that ex.args has LineNumberNodes
    args = ex.args
    n = length(args)

    # Each statement has a LineNumberNode before it
    # So for 2 statements, we should have 4 elements: LNN, stmt1, LNN, stmt2
    # Check first element is LineNumberNode
    first_is_lnn = isa(args[1], LineNumberNode)
    println(first_is_lnn)  # true

    # Return the count (2 LineNumberNodes expected)
    result = 0
    if first_is_lnn
        result = result + 1
    end
    if n >= 3 && isa(args[3], LineNumberNode)
        result = result + 1
    end
    println(result)
    @test (result) == 2.0
end

true  # Test passed
