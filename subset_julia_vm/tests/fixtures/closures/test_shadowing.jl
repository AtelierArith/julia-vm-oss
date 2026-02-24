# Test closure scoping behavior
# Issue #1738: Test that closures properly distinguish parameters from captured vars
# Issue #1743: Variable shadowing (inner parameter with same name as outer variable)

using Test

# Helper: inner parameter different from outer (no shadowing)
function make_no_shadow(x)
    function inner(y)  # y is different from x
        x + y  # x is captured, y is local parameter
    end
    inner
end

# Helper: variable shadowing - inner parameter shadows outer variable (Issue #1743)
function make_shadower(x)
    function inner(x)  # x shadows outer x
        x * 2  # should use inner x, not captured outer x
    end
    inner
end

# Helper: shadowing with additional captured variable
function make_shadow_with_capture(x)
    captured_val = x * 10  # uses outer x
    function inner(x)  # x shadows outer x
        captured_val + x  # captured_val from outer, x from inner parameter
    end
    inner
end

# Helper: multiple parameter shadowing
function make_multi_shadow(x, y)
    z = x + y  # z is local, computed from outer x and y
    function inner(x, y)  # shadows both x and y
        x + y + z  # inner's x, inner's y, captured z
    end
    inner
end

# Helper: multiple captured vars with distinct parameter
function make_multi_capture(a, b)
    function compute(x)  # x is parameter, a and b are captured
        a * x + b
    end
    compute
end

# Helper: closure capturing with arithmetic
function make_offset_fn(offset)
    function add_offset(value)
        value + offset
    end
    add_offset
end

@testset "Closure Scoping" begin
    @testset "parameter distinct from captured variable" begin
        fn = make_no_shadow(100)
        @test fn(1) == 101  # 100 + 1
        @test fn(50) == 150  # 100 + 50
    end

    @testset "multiple captured vars with parameter" begin
        f = make_multi_capture(2, 10)  # a=2, b=10
        @test f(5) == 20  # 2 * 5 + 10 = 20
        @test f(0) == 10  # 2 * 0 + 10 = 10
    end

    @testset "capture with simple arithmetic" begin
        add5 = make_offset_fn(5)
        add100 = make_offset_fn(100)

        @test add5(10) == 15
        @test add100(10) == 110
    end

    # Issue #1743: Variable shadowing tests
    @testset "variable shadowing - inner shadows outer" begin
        shadow_fn = make_shadower(10)
        # Should use inner x=5, not outer x=10
        @test shadow_fn(5) == 10   # 5 * 2 = 10, NOT 10 * 2 = 20
        @test shadow_fn(7) == 14   # 7 * 2 = 14
    end

    @testset "shadowing with captured variable" begin
        fn = make_shadow_with_capture(5)  # captured_val = 50
        # captured_val=50 + inner x
        @test fn(3) == 53   # 50 + 3
        @test fn(10) == 60  # 50 + 10
    end

    @testset "multiple parameter shadowing" begin
        fn = make_multi_shadow(100, 200)  # z = 300
        # inner's x + inner's y + captured z
        @test fn(10, 5) == 315   # 10 + 5 + 300
        @test fn(1, 2) == 303    # 1 + 2 + 300
    end
end

true
