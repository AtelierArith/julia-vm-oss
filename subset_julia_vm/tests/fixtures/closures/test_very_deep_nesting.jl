# Test very deeply nested closures (5+ levels) - Issue #1764
# Extends Issue #1744 coverage to verify extreme nesting depths

using Test

# Test 1: 5 levels of nesting with captures from all levels
function test_five_level_capture()
    function l0(a)
        function l1(b)
            function l2(c)
                function l3(d)
                    function l4(e)
                        a + b + c + d + e
                    end
                    l4
                end
                l3
            end
            l2
        end
        l1
    end

    f1 = l0(1)   # a = 1
    f2 = f1(2)   # b = 2
    f3 = f2(3)   # c = 3
    f4 = f3(4)   # d = 4
    result = f4(5)  # e = 5, result = 1+2+3+4+5 = 15
    result == 15
end

# Test 2: 6 levels of nesting
function test_six_level_capture()
    function l0(a)
        function l1(b)
            function l2(c)
                function l3(d)
                    function l4(e)
                        function l5(f)
                            a + b + c + d + e + f
                        end
                        l5
                    end
                    l4
                end
                l3
            end
            l2
        end
        l1
    end

    f1 = l0(1)
    f2 = f1(2)
    f3 = f2(3)
    f4 = f3(4)
    f5 = f4(5)
    result = f5(6)  # 1+2+3+4+5+6 = 21
    result == 21
end

# Test 3: Sibling closures capturing same ancestor variable
function test_sibling_capture()
    function make_outer(x)
        function mid()
            function inner1()
                x + 1
            end
            function inner2()
                x + 2
            end
            (inner1, inner2)
        end
        mid
    end

    m = make_outer(10)
    pair = m()
    i1 = pair[1]
    i2 = pair[2]
    (i1() == 11) && (i2() == 12)
end

# Test 4: Deep nesting with sibling closures at multiple levels
function test_deep_siblings()
    function l0(a)
        function l1()
            function sib1()
                a + 1
            end
            function sib2()
                function deep()
                    a + 100
                end
                deep
            end
            (sib1, sib2)
        end
        l1
    end

    l1_fn = l0(5)
    pair = l1_fn()
    s1 = pair[1]
    s2_maker = pair[2]
    s2_deep = s2_maker()
    (s1() == 6) && (s2_deep() == 105)
end

# Test 5: Very deep with shadowing at intermediate levels
function test_deep_with_shadowing()
    function l0(x)
        function l1(x)  # shadows l0's x
            function l2()
                function l3()
                    x  # should be l1's x
                end
                l3
            end
            l2
        end
        l1
    end

    f1 = l0(100)   # outer x = 100
    f2 = f1(42)    # inner x = 42 (shadows)
    f3 = f2()
    result = f3()
    result == 42   # should use l1's x, not l0's
end

@testset "Very deeply nested closures (5+ levels) (Issue #1764)" begin
    @test test_five_level_capture()
    @test test_six_level_capture()
    @test test_sibling_capture()
    @test test_deep_siblings()
    @test test_deep_with_shadowing()
end

true
