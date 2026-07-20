# Test: Implicit return handling in runtime specialization (Issue #1726)
# Prevention tests for Issue #1719 - ensures implicit returns work correctly
# when if statements are the last statement in a function body,
# especially with untyped Bool parameters that trigger runtime specialization.

using Test

# Test 1: Basic untyped Bool if statement as implicit return
# When a function parameter is untyped and receives a Bool,
# the specializer must handle implicit returns correctly.
function test_untyped_bool_return(flag)
    if flag
        1
    else
        0
    end
end

# Test 2: Nested if as implicit return
function test_nested_if_return(a, b)
    if a
        if b
            1
        else
            2
        end
    else
        3
    end
end

# Test 3: If without else as implicit return (returns nothing when false)
function test_no_else_return(flag)
    if flag
        42
    end
end

# Test 4: Mixed typed and untyped parameters
function test_mixed_types(x::Int64, flag)
    if flag
        x * 2
    else
        x
    end
end

# Test 5: If-elseif-else as implicit return with untyped param
function test_elseif_return(x)
    if x == 1
        "one"
    elseif x == 2
        "two"
    else
        "other"
    end
end

@testset "Implicit return in runtime specialization (Issue #1726)" begin
    # Test 1: Basic untyped Bool if/else implicit return
    @test test_untyped_bool_return(true) == 1
    @test test_untyped_bool_return(false) == 0

    # Test 2: Nested if with untyped Bool parameters
    @test test_nested_if_return(true, true) == 1
    @test test_nested_if_return(true, false) == 2
    @test test_nested_if_return(false, true) == 3
    @test test_nested_if_return(false, false) == 3

    # Test 3: If without else - returns nothing when condition is false
    @test test_no_else_return(true) == 42
    @test test_no_else_return(false) === nothing

    # Test 4: Mixed typed and untyped parameters
    @test test_mixed_types(5, true) == 10
    @test test_mixed_types(5, false) == 5
    @test test_mixed_types(0, true) == 0
    @test test_mixed_types(0, false) == 0

    # Test 5: If-elseif-else with untyped parameter
    @test test_elseif_return(1) == "one"
    @test test_elseif_return(2) == "two"
    @test test_elseif_return(3) == "other"
end

true
