# This file is a part of Julia. License is MIT: https://julialang.org/license

# =============================================================================
# Test - Unit testing module
# =============================================================================
# Pure Julia implementation using internal builtins for state management.
#
# Supported macros:
#   @test expr            - Test that expr evaluates to true
#   @test expr "message"  - Test with optional message
#   @testset "name" begin
#       @test ...
#   end
#   @test_throws ExceptionType expr - Test that expr throws an exception
#   @test_broken expr     - Test that is expected to fail (broken)
#
# NOT supported (Julia Test features not implemented):
#   @test_skip, @test_warn, @test_nowarn
#   @test_logs, @test_deprecated, @inferred
#   Custom AbstractTestSet types

module Test

# Note: Macros are registered via STDLIB_MACROS registry when `using Test` is called.
# Export statements for macros are not needed and cause issues with the current parser.

# Internal builtins (not exported):
# _test_record!(passed::Bool, msg::String) - record a test result
# _test_record_broken!(passed::Bool, msg::String) - record a broken test result
# _testset_begin!(name::String) - begin a test set
# _testset_end!() - end a test set and print summary

# @test macro: Test that an expression evaluates to true
# Usage: @test 1 + 1 == 2
# Note: @test with custom message (@test expr "msg") not yet supported
macro test(ex)
    expr_str = string(ex)
    quote
        local result = $(esc(ex))
        _test_record!(result, $expr_str)
        nothing
    end
end

# @testset macro: Group tests with a name
# Usage: @testset "name" begin ... end
macro testset(name, body)
    quote
        _testset_begin!($(esc(name)))
        $(esc(body))
        _testset_end!()
    end
end

# @test_throws macro: Test that an expression throws an exception
# Usage: @test_throws ErrorException error("oops")
#
# Note: Currently checks only if an exception was thrown, not the exact type.
# Type matching (e.g., thrown_type <: expected_type) requires subtype operator support.
#
# Limitation: Due to macro parameter evaluation semantics, string(ex) cannot be
# used outside the quote block. The test message is simplified.
macro test_throws(T, ex)
    quote
        local threw = false
        try
            $(esc(ex))
        catch e
            threw = true
        end
        if threw
            _test_record!(true, "expression throws expected exception")
        else
            _test_record!(false, "expression throws expected exception")
        end
        nothing
    end
end

# @test_broken macro: Test that is expected to fail (broken)
# Usage: @test_broken 1 == 2
#
# This macro marks a test that is expected to fail. If the test fails (returns false
# or throws), it is recorded as "Broken" (expected). If the test passes (returns true),
# it is recorded as an "Error" (unexpected pass - the test is no longer broken!).
macro test_broken(ex)
    expr_str = string(ex)
    quote
        local threw = false
        local passed = false
        try
            local result = $(esc(ex))
            # Convert result to Bool: true if truthy, false otherwise
            # Use == instead of === for compatibility
            passed = (result == true)
        catch e
            threw = true
            passed = false
        end
        if threw
            # Test threw an exception - this is expected for a broken test
            _test_record_broken!(false, $expr_str)
        else
            # Test completed - if it passed, that's unexpected (error)
            # If it failed, that's expected (broken)
            _test_record_broken!(passed, $expr_str)
        end
        nothing
    end
end

end # module Test
