# Issue #4833 (follow-up to #4784/PR #4832): nested try/catch as
# expression — where an inner `try ... catch ... end` is the last
# expression of an outer `try` body — did not propagate the inner
# value back to the outer rewrite. The inner try was treated as a
# statement inside the outer's try_block, so the outer's result
# variable stayed at its default (`nothing`) and a runtime
# `UndefVarError` was raised.
#
# Fix: made the `assign_last_value` helper in `lower_try_as_expr`
# recurse — when the last statement of a branch is itself a
# `Stmt::Try`, recursively rewrite ITS branches so the inner's
# last expression flows back to the SAME outer result variable.
# Other non-expression statements (Stmt::Return, etc.) are still
# left as-is; that's the documented falls-through path.

using Test

@testset "Nested try — inner success propagates (Issue #4833)" begin
    r = try
        try
            1
        catch
            2
        end
    catch
        3
    end
    @test r == 1
end

@testset "Nested try — inner caught propagates 'inner caught' (Issue #4833)" begin
    r = try
        try
            error("inner")
        catch
            "inner caught"
        end
    catch
        "outer caught"
    end
    @test r == "inner caught"
end

@testset "Nested try — inner re-raises, outer catches (Issue #4833)" begin
    r = try
        try
            error("first")
            "never"
        catch
            error("second")
        end
    catch e
        "outer: $(typeof(e))"
    end
    @test r == "outer: ErrorException"
end

@testset "Triple nested try — innermost success propagates (Issue #4833)" begin
    r = try
        try
            try
                42
            catch
                1
            end
        catch
            2
        end
    catch
        3
    end
    @test r == 42
end

@testset "Nested try as RHS in arithmetic (Issue #4833)" begin
    r = 100 + (try
        try
            5
        catch; 0; end
    catch; 0; end)
    @test r == 105
end

@testset "Simple (non-nested) try-as-expr still works (regression Issue #4833)" begin
    r = try
        42
    catch
        -1
    end
    @test r == 42
end

true
