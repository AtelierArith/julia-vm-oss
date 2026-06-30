# Issue #6223: a function whose final expression is a `try/catch` expression
# discarded the branch value and returned the default (`0`) instead of the
# value of whichever branch ran.
#
# Upstream Julia treats `try/catch[/else/finally]` in tail (implicit-return)
# position as an expression whose value is the last expression of whichever
# branch executed (try body if no exception, catch body if one; else body
# replaces the try value when present; finally never contributes the value).
#
# Fix: the compile-layer implicit-return path now converts a tail-position
# `Stmt::Try` into the same value-producing `Expr::LetBlock` that expression
# position already used (shared `try_stmt_into_value_expr`).

using Test

function try_value_ok()
    try
        42
    catch
        0
    end
end

function try_value_catch()
    try
        error("x")
        42
    catch
        7
    end
end

@testset "try implicit return — success path (Issue #6223)" begin
    @test try_value_ok() == 42
end

@testset "try implicit return — exception path (Issue #6223)" begin
    @test try_value_catch() == 7
end

function try_named_catch()
    try
        error("boom")
    catch e
        e isa Exception
    end
end

@testset "try implicit return — named catch var (Issue #6223)" begin
    @test try_named_catch() === true
end

function try_else_value(x)
    try
        x
    catch
        -1
    else
        100
    end
end

@testset "try implicit return — else replaces try value (Issue #6223)" begin
    @test try_else_value(5) == 100
end

function try_finally_value()
    try
        11
    finally
        # finally never contributes the returned value
        nothing
    end
end

@testset "try implicit return — finally keeps try value (Issue #6223)" begin
    @test try_finally_value() == 11
end

function try_finally_catch_value()
    try
        error("boom")
        1
    catch
        40
    finally
        nothing
    end
end

@testset "try implicit return — finally + catch value (Issue #6223)" begin
    @test try_finally_catch_value() == 40
end

function try_in_if(c)
    if c
        try
            1
        catch
            2
        end
    else
        3
    end
end

@testset "try implicit return nested in if (Issue #6223)" begin
    @test try_in_if(true) == 1
    @test try_in_if(false) == 3
end

function try_float_value()
    try
        3.5
    catch
        0.0
    end
end

@testset "try implicit return — Float64 value preserved (Issue #6223)" begin
    @test try_float_value() === 3.5
end

function try_string_value()
    try
        "ok"
    catch
        "err"
    end
end

@testset "try implicit return — String value preserved (Issue #6223)" begin
    @test try_string_value() == "ok"
end

true
