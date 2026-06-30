# Issue #5175: regression guard for dispatch_instr handler reordering.
#
# The hot dispatch path (jump / call / return / locals / arithmetic /
# comparison / stack) is exercised by tight loops, recursive function
# calls, and conditional control flow. Reordering the linear handler
# chain in `dispatch_instr` must not change any observable result.

using Test

# Recursion exercises Call + Return on every back-edge.
function fib(n)
    if n < 2
        return n
    end
    return fib(n - 1) + fib(n - 2)
end

# Nested loops exercise Jump / JumpIfZero + arithmetic + locals heavily.
function nested_sum(m)
    total = 0
    for i in 1:m
        j = 0
        while j < m
            total = total + i * j
            j = j + 1
        end
    end
    total
end

# Comparison + short-circuit control flow.
function count_even(n)
    c = 0
    for k in 1:n
        if k % 2 == 0
            c = c + 1
        end
    end
    c
end

@testset "Issue #5175 dispatch hot path" begin
    @test fib(10) == 55
    @test fib(15) == 610
    @test nested_sum(5) == 150
    @test nested_sum(8) == 1008
    @test count_even(10) == 5
    @test count_even(101) == 50
end

true  # Test passed
