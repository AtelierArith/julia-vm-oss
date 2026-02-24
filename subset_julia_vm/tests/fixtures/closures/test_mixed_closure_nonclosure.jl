# Test mixed closure and non-closure nested functions (Issue #1758)
#
# This tests scenarios where a parent function contains both:
# - Closure functions (that capture outer variables)
# - Non-closure functions (that shadow all outer variables or don't reference them)

using Test

# Test 1: Mixed closure and non-closure nested functions
function mixed_nested_fns(x)
    y = x * 10  # Local variable to be captured

    # Closure: captures y from outer scope
    function closure_fn(z)
        y + z  # y is captured, z is parameter
    end

    # Non-closure: x shadows outer x, no capture needed
    function regular_fn(x)
        x * 2  # uses parameter x, not outer x
    end

    (closure_fn, regular_fn)
end

# Test 2: Multiple nested functions with same name in different parents
function parent_a(x)
    function inner(y)
        x + y  # captures x
    end
    inner
end

function parent_b(a)
    function inner(b)
        a * b  # captures a
    end
    inner
end

# Test 3: Non-closure function called alongside closure
function mixed_calls(base)
    multiplier = base * 2  # captured by multiply_and_add

    # Closure that captures multiplier
    function multiply_and_add(x, y)
        multiplier + x + y
    end

    # Non-closure that shadows all outer vars
    function just_add(x, y)
        x + y  # no capture
    end

    # Return results of calling both
    (multiply_and_add(1, 2), just_add(10, 20))
end

# Test 4: Chain of closures and non-closures
function chain_test(initial)
    step1 = initial + 10

    # Closure capturing step1
    function closure1(x)
        step1 + x
    end

    # Non-closure (shadows variable names)
    function nonclosure(step1)
        step1 * 2  # uses parameter, not outer step1
    end

    # Closure capturing step1 again
    function closure2(x)
        step1 * x
    end

    (closure1(5), nonclosure(100), closure2(3))
end

@testset "Mixed Closure and Non-closure Nested Functions" begin
    @testset "mixed closure and regular function" begin
        (closure_fn, regular_fn) = mixed_nested_fns(5)  # y = 50

        # Closure: y(50) + z
        @test closure_fn(3) == 53
        @test closure_fn(10) == 60

        # Non-closure: x * 2 (uses parameter, not captured)
        @test regular_fn(7) == 14
        @test regular_fn(100) == 200
    end

    @testset "same name nested functions in different parents" begin
        inner_a = parent_a(10)
        inner_b = parent_b(10)

        # parent_a's inner: 10 + y
        @test inner_a(5) == 15

        # parent_b's inner: 10 * b
        @test inner_b(5) == 50

        # They should be independent
        @test inner_a(5) != inner_b(5)
    end

    @testset "calling mixed functions" begin
        result = mixed_calls(5)  # multiplier = 10

        # multiply_and_add: 10 + 1 + 2 = 13
        @test result[1] == 13

        # just_add: 10 + 20 = 30
        @test result[2] == 30
    end

    @testset "chain of closures and non-closures" begin
        result = chain_test(100)  # step1 = 110

        # closure1: step1(110) + 5 = 115
        @test result[1] == 115

        # nonclosure: parameter(100) * 2 = 200
        @test result[2] == 200

        # closure2: step1(110) * 3 = 330
        @test result[3] == 330
    end
end

true
