using Test

# Test closures inside functions that contain loops (Issue #2241).
# Previously, the abstract interpreter's return type inference treated
# loop body implicit values as function returns, causing the compiler
# to assign an incorrect type to the closure-returning function.

# Closure with while loop
function make_while_closure()
    x = 42
    i = 0
    while i < 3
        i = i + 1
    end
    function get_x()
        x
    end
    get_x
end

# Closure with for-loop over range
function make_for_range_closure()
    x = 100
    for i in 1:5
    end
    function get_x()
        x
    end
    get_x
end

# Closure with for-loop over varargs
function make_adder(args...)
    total = 0
    for x in args
        total = total + x
    end
    function adder(y)
        total + y
    end
    adder
end

@testset "Closure with loops (Issue #2241)" begin
    # While loop + closure
    f1 = make_while_closure()
    @test f1() == 42

    # For-loop (range) + closure
    f2 = make_for_range_closure()
    @test f2() == 100

    # For-loop (varargs) + closure
    f3 = make_adder(1, 2, 3)
    @test f3(10) == 16
    @test f3(0) == 6
end

true
