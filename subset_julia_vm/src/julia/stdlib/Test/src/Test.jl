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
#   @test_throws ExceptionType expr - Test that expr throws a matching exception.
#       `ExceptionType` may be a Type (`isa` check), an exception value (type
#       + field equality), a String/Regex (message substring/match), or an
#       Array/Tuple/Function combinator of those, matching upstream
#       `do_test_throws` (Issue #10354).
#   @test_broken expr     - Test that is expected to fail (broken)
#
# `@test` catches exceptions thrown while evaluating its expression (and
# non-Boolean results) and records the test as "errored" — a distinct outcome
# from "failed", matching upstream `do_test`'s `Returned`/`Threw` handling —
# so the enclosing `@testset` still runs to its summary and the process still
# exits non-zero (Issue #10093).
#
#   @test_skip expr        - Skip a test, record it as Broken without evaluating
#
# NOT supported (Julia Test features not implemented):
#   @test_warn, @test_nowarn
#   @test_logs, @test_deprecated, @inferred
#   Custom AbstractTestSet types

module Test

# Note: Macros are registered via STDLIB_MACROS registry when `using Test` is called.
# Export statements for macros are not needed and cause issues with the current parser.

# Minimal upstream-shaped result hierarchy. The recorder builtins still own the
# counters and diagnostics, while macros return these values just like Test.jl
# instead of erasing every expansion to `nothing` (Issue #10496 / #10307).
abstract type Result end
struct Pass <: Result
    marker::Nothing
end
struct Fail <: Result
    marker::Nothing
end
struct Error <: Result
    marker::Nothing
end
struct Broken <: Result
    marker::Nothing
end

abstract type AbstractTestSet end
struct DefaultTestSet <: AbstractTestSet
    marker::Nothing
end

function _test_result(kind::Int)
    kind == 0 ? Pass(nothing) : kind == 1 ? Fail(nothing) : kind == 2 ? Error(nothing) : Broken(nothing)
end

function _default_testset_result()
    DefaultTestSet(nothing)
end

export _test_result, _default_testset_result
# _test_throws_* helpers are called from inside `@test_throws`'s quote
# expansion, which runs in the CALLING module's scope (after `esc`/hygiene),
# not literally inside `module Test`. Like `_test_result` above, they must be
# exported so `using Test` brings them into scope there (Issue #10354).
export _test_throws_matches, _test_throws_describe, _test_throws_thrown_describe

# Internal builtins (not exported):
# _test_record!(passed::Bool, msg::String) - record a test result
# _test_record_broken!(passed::Bool, msg::String) - record a broken test result
# _test_record_error!(msg::String, detail::String) - record an errored test result
# _testset_begin!(name::String) - begin a test set
# _testset_end!() - end a test set and print summary

# @test macro: Test that an expression evaluates to true
# Usage: @test 1 + 1 == 2
# Note: @test with custom message (@test expr "msg") not yet supported
#
# Evaluation is wrapped in try/catch, mirroring upstream `Test.@test`
# (stdlib/Test/src/Test.jl `get_test_result` builds a `Returned`/`Threw`
# result inside `try ... catch`, and `do_test` records `Threw` as an
# `Error(:test_error, ...)` and a non-Bool `Returned` value as an
# `Error(:test_nonbool, ...)`). An exception thrown by the test expression is
# therefore recorded as an "errored" outcome instead of propagating out of
# the enclosing `@testset`, so the testset summary still prints and the run
# still exits non-zero (Issue #10093).
# Quote-internal locals below (including the `catch` variable) use natural,
# upstream-style names: the static stdlib-macro quote expansion now
# hygiene-renames every non-escaped local it introduces (gensym'd, so the
# compiled `catch e` binding is unreachable under the spelling `e` outside
# this expansion), so they can no longer collide with a user/global variable
# of the same name -- e.g. Base.MathConstants.e stays resolvable to its own
# value even though this expansion also binds a local named `e` (Issue
# #10242; previously worked around with `__test_*`-prefixed names -- see
# docs/vm/WORKAROUNDS.md W-67, Resolved).
macro test(ex)
    expr_str = string(ex)
    quote
        local threw = false
        local detail = ""
        local result = false
        local recorded = 0
        try
            result = $(esc(ex))
        catch e
            threw = true
            detail = string("Test threw exception: ", sprint(showerror, e))
        end
        if threw
            _test_record_error!($expr_str, detail)
            recorded = 2
        elseif result isa Bool
            _test_record!(result, $expr_str)
            if result
                recorded = 0
            else
                recorded = 1
            end
        else
            _test_record_error!(
                $expr_str,
                string("Expression evaluated to non-Boolean: ", repr(result)),
            )
            recorded = 2
        end
        _test_result(recorded)
    end
end

# @testset macro: Group tests with a name
# Usage: @testset "name" begin ... end
#
# The body is wrapped in a bare `let ... end` so it runs in a hard (local)
# scope, matching upstream `Test.@testset` (stdlib/Test/src/Test.jl
# `testset_beginend_call`, which wraps `$(esc(tests))` in `let ... end`).
# This makes assignments in the testset body testset-local: a `for` loop that
# accumulates into a body-local variable updates the enclosing local instead of
# hitting file-mode soft-scope localization (Issue #9312), and body variables
# do not leak into the enclosing/global scope.
macro testset(name, body)
    quote
        _testset_begin!($(esc(name)))
        let
            $(esc(body))
        end
        _testset_end!()
        _default_testset_result()
    end
end

# _test_throws_matches(expected, exc)::Bool - does the caught exception `exc`
# satisfy the `@test_throws expected ...` expectation? Mirrors upstream
# `do_test_throws` (stdlib/Test/src/Test.jl), dispatching on the RUNTIME type
# of `expected` the same way upstream's `isa(extype, ...)` chain does (Issue
# #10354):
#
#   - `expected::Type`      -> `exc isa expected` (the common `@test_throws
#     BoundsError f()` form).
#   - `expected::Exception`  -> a concrete exception VALUE (not a type): the
#     same exception type AND every field equal (`isequal`), mirroring
#     upstream `isequalexception`. Lets a test pin e.g.
#     `@test_throws UndefVarError(:x) ...` down to the exact `var`.
#   - `expected::AbstractString` / `::Regex` -> the displayed error message
#     (`sprint(showerror, exc)`) contains the substring / matches the regex.
#   - `expected::Union{Tuple,AbstractArray}` -> every element must match
#     (upstream: "a list of strings occurring in the displayed error
#     message").
#   - `expected::Function` -> called on the displayed message, must return
#     `true` (upstream: "a matching function").
#
# Not ported: upstream's `InterruptException` rethrow guard (no interactive
# signal handling in the batch VM) and the deprecated `LoadError`/`extype ==
# ErrorException && exc isa FieldError` compatibility shims (no legacy
# callers in this codebase to preserve compatibility for).
_test_throws_matches(expected::Type, exc) = exc isa expected

_test_throws_matches(expected::AbstractString, exc) =
    occursin(expected, sprint(showerror, exc))

_test_throws_matches(expected::Regex, exc) =
    occursin(expected, sprint(showerror, exc))

_test_throws_matches(expected::Function, exc) =
    expected(sprint(showerror, exc)) === true

function _test_throws_matches(expected::Union{Tuple,AbstractArray}, exc)
    for item in expected
        if !_test_throws_matches(item, exc)
            return false
        end
    end
    return true
end

function _test_throws_matches(expected::Exception, exc)
    if typeof(exc) !== typeof(expected)
        return false
    end
    n = nfields(expected)
    for i in 1:n
        if !isequal(getfield(exc, i), getfield(expected, i))
            return false
        end
    end
    return true
end

# _test_throws_describe(expected) - human-readable "Expected: ..." text for a
# `@test_throws` Fail/Pass message. A `Type` prints as its name (`ArgumentError`);
# anything else (a String, Regex, exception value, array/function) prints via
# `repr` so the message shows exactly what was written in the test.
_test_throws_describe(expected::Type) = string(expected)
_test_throws_describe(expected) = repr(expected)

# _test_throws_thrown_describe(exc) - "Thrown: ..." text: the exception's type
# and its upstream-formatted display, so a Fail message names both what was
# expected and what actually happened (Issue #10354).
_test_throws_thrown_describe(exc) = string(typeof(exc), ": ", sprint(showerror, exc))

# @test_throws macro: Test that an expression throws an exception matching
# `expected` (a Type, an exception value, a String/Regex message match, or an
# Array/Tuple/Function combinator of those -- see `_test_throws_matches`
# above). Mirrors upstream `Test.@test_throws` (stdlib/Test/src/Test.jl):
#
#   - No exception thrown -> Fail ("did not throw an exception").
#   - Exception thrown but does not match `expected` -> Fail, naming both the
#     expected and actual exception like upstream's `Expected: T / Thrown: U`
#     (Issue #10354; previously recorded Pass unconditionally, a detection
#     blind spot that hid 13 genuine sjulia bugs behind it -- see
#     docs/vm/EXCEPTION_PARITY.md).
#   - Exception thrown and matches -> Pass.
#
# `catch e` uses a natural name: the catch variable is hygiene-renamed by the
# static stdlib-macro quote expansion, so it cannot shadow a user/global `e`
# (Issue #10242; previously worked around with an `__test_*`-prefixed catch
# variable -- see docs/vm/WORKAROUNDS.md W-67, Resolved).
macro test_throws(T, ex)
    quote
        local recorded = 1
        try
            $(esc(ex))
            _test_record!(
                false,
                string(
                    "did not throw an exception; Expected: ",
                    _test_throws_describe($(esc(T))),
                ),
            )
            recorded = 1
        catch e
            if _test_throws_matches($(esc(T)), e)
                _test_record!(
                    true,
                    string(
                        "expression throws expected exception; Thrown: ",
                        _test_throws_thrown_describe(e),
                    ),
                )
                recorded = 0
            else
                _test_record!(
                    false,
                    string(
                        "wrong exception type thrown; Expected: ",
                        _test_throws_describe($(esc(T))),
                        " / Thrown: ",
                        _test_throws_thrown_describe(e),
                    ),
                )
                recorded = 1
            end
        end
        _test_result(recorded)
    end
end

# @test_broken macro: Test that is expected to fail (broken)
# Usage: @test_broken 1 == 2
#
# This macro marks a test that is expected to fail. If the test fails (returns false
# or throws), it is recorded as "Broken" (expected). If the test passes (returns true),
# it is recorded as an "Error" (unexpected pass - the test is no longer broken!).
# Quote-internal locals (including the catch variable) use natural names:
# the static stdlib-macro quote expansion hygiene-renames every non-escaped
# local it introduces, so they cannot collide with a user/global variable of
# the same name (Issue #10242; previously worked around with `__test_*`-
# prefixed names -- see docs/vm/WORKAROUNDS.md W-67, Resolved).
macro test_broken(ex)
    expr_str = string(ex)
    quote
        local threw = false
        local passed = false
        local recorded = 3
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
            recorded = 3
        else
            # Test completed - if it passed, that's unexpected (error)
            # If it failed, that's expected (broken)
            _test_record_broken!(passed, $expr_str)
            if passed
                recorded = 2
            else
                recorded = 3
            end
        end
        _test_result(recorded)
    end
end

# @test_skip macro: Skip a test, recording it as Broken without evaluating it
# Usage: @test_skip 1 == 2
#
# Mirrors upstream `Test.@test_skip` (stdlib/Test/src/Test.jl): the expression
# is NOT evaluated (so a throwing expression is fine) and the test is recorded
# as a Broken outcome (`Broken(:skipped, ex)`), which never fails the run
# (Issue #10350). Routed through the same `_test_record_broken!` recorder as
# `@test_broken` so the unified-harness invariant holds (Issue #10273).
macro test_skip(ex)
    expr_str = string(ex)
    quote
        _test_record_broken!(false, $expr_str)
        nothing
    end
end

end # module Test
