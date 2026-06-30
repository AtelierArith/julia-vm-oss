# Issue #4784: `try/catch/end` could not be used as an expression
# (assignment RHS, inside arithmetic, etc.) — only as a statement.
# Upstream Julia treats it as a first-class expression whose value
# is the last expression in whichever branch ran (try body if no
# exception, catch body if one).
#
# Fix:
# 1. Parser: added `Token::KwTry` to the primary-expression dispatch
#    in `parser/expressions/primary.rs`, mirroring how `KwIf`,
#    `KwLet`, etc. are accepted as expressions.
# 2. Lowering: added a `NodeKind::TryStatement` arm to `lower_expr`
#    that wraps the lowered `Stmt::Try` in an `Expr::LetBlock`,
#    rewriting the last `Stmt::Expr` of each branch (try / catch /
#    else) into an `Stmt::Assign` to a fresh result variable, and
#    yielding that variable as the LetBlock's final value.
#
# Out of scope (separate inference bug): `parse(Int, s)` where `s`
# is an untyped function parameter fails compilation independently
# of try/catch; that's why the fixture uses literal arguments to
# parse instead of function parameters.

using Test

@testset "try/catch as assignment RHS — success path (Issue #4784)" begin
    r = try
        42
    catch
        -1
    end
    @test r == 42
end

@testset "try/catch as assignment RHS — exception path (Issue #4784)" begin
    r = try
        error("oops")
    catch
        100
    end
    @test r == 100
end

@testset "try/catch with named catch var (Issue #4784)" begin
    r = try
        error("test")
    catch e
        e isa Exception
    end
    @test r === true
end

@testset "try/catch inside arithmetic (Issue #4784)" begin
    r = 1 + (try 10 catch; 0 end)
    @test r == 11
end

@testset "try/catch inside arithmetic — exception path (Issue #4784)" begin
    r = 100 + (try error("oops") catch; 5 end)
    @test r == 105
end

@testset "try/catch as statement still works (regression Issue #4784)" begin
    # Bare statement form must still compile and execute.
    captured = false
    try
        captured = true
    catch
        captured = false
    end
    @test captured === true
end

true
