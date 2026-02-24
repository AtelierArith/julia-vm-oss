# Test deeply nested closures (3+ levels) - Issue #1744
# Verifies that closures can capture variables from multiple ancestor scopes

using Test

# Test 1: 3 levels of nesting with single capture
function test_three_level_single_capture()
    function make_outermost(x)
        function middle()
            function inner()
                x
            end
            inner
        end
        middle
    end

    m = make_outermost(42)
    i = m()
    result = i()
    result == 42
end

# Test 2: 4 levels of nesting with captures from all levels
function test_four_level_multi_capture()
    function make_deep(a)
        function level1(b)
            function level2(c)
                function level3(d)
                    a + b + c + d
                end
                level3
            end
            level2
        end
        level1
    end

    l1 = make_deep(1)
    l2 = l1(2)
    l3 = l2(3)
    result = l3(4)
    result == 10  # 1+2+3+4
end

# Test 3: Mixed parameter and captured variables at each level
function test_mixed_captures()
    function outermost(a)
        function mid(b)
            z = a * b  # local variable in mid
            function deep(c)
                a + z + c  # captures a and z from different levels
            end
            deep
        end
        mid
    end

    f = outermost(2)   # a = 2
    g = f(3)       # b = 3, z = 6
    result = g(4)  # c = 4, result = 2 + 6 + 4 = 12
    result == 12
end

# Test 4: Return closures that capture different variables
function test_different_captures_same_level()
    function make_getters(x, y)
        function level1()
            function get_x()
                x
            end
            function get_y()
                y
            end
            (get_x, get_y)
        end
        level1
    end

    getters = make_getters(10, 20)
    pair = getters()
    gx = pair[1]
    gy = pair[2]
    (gx() == 10) && (gy() == 20)
end

@testset "Deeply nested closures (Issue #1744)" begin
    @test test_three_level_single_capture()
    @test test_four_level_multi_capture()
    @test test_mixed_captures()
    @test test_different_captures_same_level()
end

true
