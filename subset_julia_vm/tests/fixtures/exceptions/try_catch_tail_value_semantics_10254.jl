# Issue #10254 (design/prevention): lock in try/catch/finally tail-position
# value-propagation semantics end-to-end, as a design close-out for the
# concrete bug fixed in Issue #10074.
#
# Upstream Julia rule (this fixture's contract):
#   1. `try ... catch ... end` is itself an expression. Its value is the
#      value of the LAST expression of whichever branch actually ran.
#   2. An assignment `x = v` (plain, op-assign, or `global x = v`) is
#      itself an expression whose value is the assigned value `v` — so a
#      branch that ends in an assignment still produces a value.
#   3. `finally` NEVER contributes the produced value; it runs purely for
#      its side effect, and the try/catch value is whatever it was before
#      `finally` ran.
#   4. This holds uniformly whether the try/catch sits in:
#        - implicit-return (tail) position of a function body,
#        - a top-level or local rvalue position (`r = try ... end`),
#        - nested inside another try/catch or an if branch.
#   5. The declared/inferred return type is the JOIN of every branch's
#      tail type, so callers can use the result immediately (arithmetic,
#      typeof, string concatenation, ::T-annotated return) without a
#      separate widening step.
#
# Mechanism (see docs/vm/LOWERING.md "try/catch/finally as an expression"
# and docs/vm/TYPE_SYSTEM.md for the full writeup):
#   - `try_stmt_into_value_expr` / `if_stmt_into_value_expr`
#     (lowering/expr/mod.rs) rewrite a `Stmt::Try`/`Stmt::If` used in value
#     position into an `Expr::LetBlock` whose body assigns each branch's
#     tail value into a shared fresh result variable via the shared
#     `assign_block_tail_value` helper, then reads that variable back as
#     the LetBlock's value. `finally_block` is left untouched by this
#     rewrite, so it can never feed the result variable.
#   - `assign_block_tail_value` recognizes a trailing `Stmt::Expr`,
#     `Stmt::Assign`, `Stmt::AddAssign`, or a trailing nested
#     `Stmt::Try`/`Stmt::If`/`Stmt::Block` (recursing into ITS tail) so the
#     "last statement of a block is its value" rule composes through
#     nesting.
#   - `compile_block_value` (compile/expr/mod.rs) is the codegen twin:
#     compiling the empty-binding `Expr::LetBlock` that a `begin ... end`
#     lowers to, with the matching arms for a trailing assignment.
#   - Return-type inference (`infer_block_branch` in
#     compile/abstract_interp/engine/mod.rs, `Stmt::Try` arms in
#     compile/inference.rs) independently joins the try-branch and
#     catch/else-branch tail types, so the type seen by the caller matches
#     the value actually produced at runtime.
#
# This fixture is the #10254 design close-out: it exercises the matrix
# above (assign-only branches, rvalue position vs. function-tail position,
# nested try/catch, catch-var binding, finally-does-not-override, typed
# post-use) as a single regression suite, complementing (not duplicating)
# the narrower fixtures already covering pieces of this:
#   - try_catch_expression_4784.jl       (bare-value expression position)
#   - nested_try_catch_expression_4833.jl (nested, bare-value tails)
#   - try_implicit_return_6223.jl        (bare-value tail/implicit-return)
#   - try_catch_type_inference_9131.jl   (env-join / return-type coverage)
#   - try_tail_assign_implicit_return_10074.jl (assign-tail, function-tail
#     position only)
# The gap this fixture closes: assign-tail branches at RVALUE/expression
# position (not just function-tail position), nested try/catch with
# assign-only tails, catch-var-binding + assign tail, try/finally without
# a catch clause with an assign-only tail, and typed post-use (arithmetic,
# typeof, string concat, `::T` return annotation) of an assign-tail result.

using Test

# ── 1. Assign-only branches, function tail position (Issue #10074 MWE) ──

function tail_assign_only()
    try
        x = 5
    catch
        x = -1
    end
end

@testset "assign-only branches, function tail position (Issue #10254/#10074)" begin
    @test tail_assign_only() == 5
end

# ── 2. Mid-body value in try, catch assign; both branches exercised ────

function mid_body_then_assign(e)
    try
        e && error("b")
        y = 10
    catch
        y = -1
    end
end

@testset "mid-body statement then tail assign — try branch (Issue #10254)" begin
    @test mid_body_then_assign(false) == 10
end

@testset "mid-body statement then tail assign — catch branch (Issue #10254)" begin
    @test mid_body_then_assign(true) == -1
end

# ── 3. try/catch AS AN RVALUE with an assign-only tail (the gap not ─────
#    covered by try_catch_expression_4784.jl, which only uses bare-value
#    tails, or try_tail_assign_implicit_return_10074.jl, which only tests
#    function-tail/implicit-return position) ─────────────────────────────

function rvalue_tail_assign()
    y = try
        a = 5
    catch
        a = -1
    end
    return y
end

@testset "try/catch as rvalue with assign-only tail, local (Issue #10254)" begin
    @test rvalue_tail_assign() == 5
end

# Module-level (top-level) rvalue form — not nested in any function.
r_toplevel_assign_tail = try
    q = 42
catch
    q = -1
end

@testset "try/catch as rvalue with assign-only tail, top-level (Issue #10254)" begin
    @test r_toplevel_assign_tail == 42
end

# ── 4. try/finally: finally must NEVER override the produced value, ────
#    both when finally has its own trailing value and when the try
#    branch's tail is itself a plain assignment.

function finally_value_not_returned()
    try
        1
    finally
        99
    end
end

@testset "finally's own tail value never becomes the result (Issue #10254)" begin
    @test finally_value_not_returned() == 1
end

function finally_assign_only()
    try
        x = 5
    finally
        c = 0
    end
end

@testset "try/finally (no catch) assign-only tail (Issue #10254)" begin
    @test finally_assign_only() == 5
end

# try/finally (no catch clause) with an assign tail, then used arithmetically
# by the caller — exercises both value AND inferred-type correctness.
function finally_no_catch_typed_use()
    z = try
        z_inner = 3
    finally
        nothing
    end
    return z
end

@testset "try/finally (no catch) assign tail, typed arithmetic post-use (Issue #10254)" begin
    @test finally_no_catch_typed_use() * 2 == 6
end

# ── 5. Nested try/catch, assign-only tails at every level ──────────────

function nested_assign_tails(e)
    try
        try
            e && error("x")
            w = 1
        catch
            w = 2
        end
    catch
        w = 3
    end
end

@testset "nested try/catch, assign-only tails — inner try branch (Issue #10254)" begin
    @test nested_assign_tails(false) == 1
end

@testset "nested try/catch, assign-only tails — inner catch branch (Issue #10254)" begin
    @test nested_assign_tails(true) == 2
end

# ── 6. catch with a bound exception variable, assign-only tail ─────────

function catch_binding_assign_tail()
    try
        error("boom")
    catch e
        m = 7
    end
end

@testset "catch with bound exception var, assign-only tail (Issue #10254)" begin
    @test catch_binding_assign_tail() == 7
end

# ── 7. global assign tail, both in a plain try/finally and in try/catch, ─
#    plus typed arithmetic post-use of the caller-visible return value.

global gz10254 = 0

function global_assign_tail_finally()
    try
        global gz10254 = 7
    finally
    end
end

@testset "global-assign tail with finally, return value (Issue #10254)" begin
    @test global_assign_tail_finally() == 7
end

@testset "global-assign tail with finally, module binding updated (Issue #10254)" begin
    @test gz10254 == 7
end

function global_assign_tail_try_catch(cond)
    try
        global gg10254 = cond ? error("boom") : 5
    catch
        global gg10254 = -1
    end
end

@testset "global-assign tail in try/catch, typed arithmetic post-use (Issue #10254)" begin
    @test global_assign_tail_try_catch(false) + 1 == 6
    @test global_assign_tail_try_catch(true) + 1 == 0
end

# ── 8. Return type correctness: caller uses the value in arithmetic and ──
#    typeof(), not just equality — this exercises return-type inference
#    (the join of try/catch branch tail types), not merely the runtime
#    value.

function inferred_int_result()
    try
        x = 5
    catch
        x = -1
    end
end

@testset "caller arithmetic post-use of assign-tail result (Issue #10254)" begin
    @test inferred_int_result() + 100 == 105
end

@testset "caller typeof() of assign-tail result (Issue #10254)" begin
    @test typeof(inferred_int_result()) == Int64
end

# ── 9. String-typed assign tail, used with string concatenation by the ───
#    caller (exercises non-numeric join + post-use).

function string_tail_result(b)
    try
        b && error("e")
        s = "ok"
    catch
        s = "err"
    end
end

@testset "String-typed assign tail, concatenation post-use — try branch (Issue #10254)" begin
    @test string_tail_result(false) * "!" == "ok!"
end

@testset "String-typed assign tail, concatenation post-use — catch branch (Issue #10254)" begin
    @test string_tail_result(true) * "!" == "err!"
end

# ── 10. `::Int` return annotation on a function whose body is a bare ─────
#     try/catch with assign-only tails on both branches.

function annotated_int_return()::Int
    try
        y = 10
    catch
        y = 0
    end
end

@testset "::Int-annotated return, try/catch assign-only tails (Issue #10254)" begin
    @test annotated_int_return() + 1 == 11
end

true
