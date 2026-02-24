using Test

# Test closures inside functions that contain nested loops (Issue #2255).
# This extends the Issue #2241 test to verify nested loop bodies
# don't interfere with closure return type inference.

# Closure with nested loops (for inside while)
function make_nested_closure()
    x = 42
    i = 0
    while i < 2
        for j in 1:3
            # Inner loop body
        end
        i = i + 1
    end
    function get_x()
        x
    end
    get_x
end

# Closure with deeply nested loops
function make_deep_nested_closure()
    result = 0
    for i in 1:2
        for j in 1:2
            for k in 1:2
                result = result + 1
            end
        end
    end
    function get_result()
        result
    end
    get_result
end

# Explicit return inside loop body (must still work)
function find_first(arr, pred)
    for x in arr
        if pred(x)
            return x
        end
    end
    return nothing
end

# Closure after function with explicit return in loop
function make_closure_after_early_return()
    captured = 99
    # This function has explicit return in loop
    function inner_find(arr)
        for x in arr
            if x > 5
                return x
            end
        end
        return nothing
    end
    # Return a closure that captures the variable
    function get_captured()
        captured
    end
    get_captured
end

@testset "Nested loops with closure (Issue #2255)" begin
    # Nested loop + closure
    f1 = make_nested_closure()
    @test f1() == 42

    # Deeply nested loops + closure
    f2 = make_deep_nested_closure()
    @test f2() == 8  # 2 * 2 * 2 = 8 iterations

    # Explicit return inside loop
    @test find_first([1, 2, 3, 4, 5, 6], x -> x > 4) == 5
    @test find_first([1, 2, 3], x -> x > 10) === nothing

    # Closure after early return function
    f3 = make_closure_after_early_return()
    @test f3() == 99
end

true
