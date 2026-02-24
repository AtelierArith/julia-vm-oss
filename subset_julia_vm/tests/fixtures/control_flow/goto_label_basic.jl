# Basic @goto/@label control flow
# Tests the fundamental goto/label functionality

using Test

# Test 1: Basic forward jump
function test_forward_jump()
    x = 0
    x = 1
    @goto skip
    x = 999  # This should be skipped
    @label skip
    x += 10
    return x
end

# Test 2: Backward jump (simple loop with goto)
function test_backward_jump()
    sum = 0
    i = 0
    @label start
    i += 1
    sum += i
    if i < 5
        @goto start
    end
    return sum
end

# Test 3: Multiple labels
function test_multiple_labels()
    result = 0
    x = 2

    if x == 1
        @goto label_one
    elseif x == 2
        @goto label_two
    else
        @goto label_three
    end

    @label label_one
    result = 1
    @goto done

    @label label_two
    result = 2
    @goto done

    @label label_three
    result = 3
    @goto done

    @label done
    return result
end

# Test 4: Nested control with goto
function test_nested_goto()
    total = 0
    for i in 1:10
        if i == 5
            @goto after_loop
        end
        total += i
    end
    @label after_loop
    return total
end

@testset "@goto/@label basic control flow" begin
    # Forward jump: x should be 1 + 10 = 11
    @test (test_forward_jump()) == 11.0

    # Backward jump: sum of 1+2+3+4+5 = 15
    @test (test_backward_jump()) == 15.0

    # Multiple labels with x=2 should go to label_two, result = 2
    @test (test_multiple_labels()) == 2.0

    # Nested: sum 1+2+3+4 = 10 (skips i=5 onwards)
    @test (test_nested_goto()) == 10.0
end

true  # Test passed
